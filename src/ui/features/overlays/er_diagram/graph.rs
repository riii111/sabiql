use std::collections::{BTreeMap, HashMap, HashSet};

use rataflow::{Edge, Flow, HandlePosition, Node, Position, Sugiyama};
use unicode_width::UnicodeWidthStr;

use super::content::{
    EDGE_APPROACH, EDGE_STEM, FkEdge, SELF_REFERENCE_MARK, TableNode, departure_point, facing_sides,
};
use crate::app::model::er_state::ErLayout;
use crate::domain::ErTableInfo;

pub(super) type ErFlow = Flow<TableNode, FkEdge>;

const NODE_SPACING: f64 = 2.0;
const MIN_RANK_SPACING: usize = 8;
const MAX_RANK_SPACING: usize = 40;
// Distance between the turn-in lines of tables entered from one column side.
const TRACK_SPACING: f64 = 2.0;
// Turn-in lines a gap between columns fits beside the longest label.
const TRACKS_BESIDE_LABELS: f64 = 2.0;
const COMPONENT_GAP: f64 = 4.0;
const COMPONENT_COLUMN_GAP: f64 = COMPONENT_GAP * 2.0;
// rataflow's Sugiyama layout grows superlinearly (seconds past ~1000 tables);
// beyond this, tables are packed in a grid instead.
pub(super) const LAYERED_LAYOUT_MAX_TABLES: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Relation {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) fk_names: Vec<String>,
}

pub(super) fn collect_relations(tables: &[ErTableInfo]) -> Vec<Relation> {
    let mut grouped: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    for fk in tables.iter().flat_map(|table| &table.foreign_keys) {
        grouped
            .entry((fk.from_qualified.as_str(), fk.to_qualified.as_str()))
            .or_default()
            .push(fk.name.clone());
    }
    grouped
        .into_iter()
        .map(|((from, to), mut fk_names)| {
            fk_names.sort();
            fk_names.dedup();
            Relation {
                from: from.to_string(),
                to: to.to_string(),
                fk_names,
            }
        })
        .collect()
}

/// Positions every table for the viewer.
///
/// Left to right with rataflow's Sugiyama layout (a plain grid for very large
/// schemas), then the disconnected parts are packed into rows. Runs on a
/// blocking task because it is slow for large schemas.
pub fn layout_tables(tables: &[ErTableInfo]) -> ErLayout {
    let relations = collect_relations(tables);
    let Ok(mut flow) = table_graph(tables, &relations) else {
        return ErLayout::default();
    };

    let layered = flow.nodes().count() <= LAYERED_LAYOUT_MAX_TABLES;
    if layered {
        // Room for the label on an edge's lane between its stem and approach,
        // plus a few tracks.
        let rank_spacing = flow
            .edges()
            .iter()
            .map(|edge| edge.content.label.width())
            .max()
            .map_or(MIN_RANK_SPACING, |width| {
                width
                    + TRACKS_BESIDE_LABELS.mul_add(TRACK_SPACING, EDGE_STEM + EDGE_APPROACH)
                        as usize
                    + 1
            })
            .clamp(MIN_RANK_SPACING, MAX_RANK_SPACING);
        flow.apply_layout(
            Sugiyama::horizontal()
                .with_rank_spacing(rank_spacing as f64)
                .with_node_spacing(NODE_SPACING),
        );
    }
    pack_components(&mut flow, layered);

    ErLayout {
        positions: flow
            .nodes()
            .map(|node| {
                let position = (node.position.x.round(), node.position.y.round());
                (node.id.clone(), position)
            })
            .collect(),
    }
}

/// The tables and FK edges, all still at the origin.
pub(super) fn table_graph(
    tables: &[ErTableInfo],
    relations: &[Relation],
) -> Result<ErFlow, rataflow::Error> {
    let mut self_references: HashMap<&str, &[String]> = HashMap::new();
    for r in relations.iter().filter(|r| r.from == r.to) {
        self_references.insert(r.from.as_str(), &r.fk_names);
    }
    let multi_schema = tables
        .iter()
        .map(|table| table.schema.as_str())
        .collect::<HashSet<_>>()
        .len()
        > 1;
    let mut seen = HashSet::new();
    let nodes: Vec<Node<TableNode>> = tables
        .iter()
        .filter(|table| seen.insert(table.qualified_name.as_str()))
        .map(|table| {
            let fk_names = self_references
                .get(table.qualified_name.as_str())
                .copied()
                .unwrap_or_default();
            table_node(table, fk_names, multi_schema)
        })
        .collect();
    let edges = relation_edges(relations, |id| seen.contains(id));
    ErFlow::with_graph(nodes, edges)
}

fn table_node(
    table: &ErTableInfo,
    self_reference_fks: &[String],
    show_schema: bool,
) -> Node<TableNode> {
    let content = TableNode {
        name: table.name.clone(),
        schema: show_schema.then(|| table.schema.clone()),
        self_reference_fks: self_reference_fks.to_vec(),
    };
    let width = content
        .self_reference_fks
        .iter()
        .map(|name| SELF_REFERENCE_MARK.width() + 1 + name.width())
        .chain([content.name.width()])
        .chain(content.schema.iter().map(|schema| schema.width() + 2))
        .max()
        .unwrap_or(0)
        + 4;
    let height = content.self_reference_fks.len() + 3;

    Node::new(
        table.qualified_name.clone(),
        (0.0, 0.0),
        (width as f64, height as f64),
        content,
    )
}

/// One edge per table pair: parallel FKs share a label, and mutual references
/// become a single double-headed edge since both directions would overlap.
fn relation_edges(relations: &[Relation], is_known: impl Fn(&str) -> bool) -> Vec<Edge<FkEdge>> {
    let by_pair: HashMap<(&str, &str), &Relation> = relations
        .iter()
        .map(|r| ((r.from.as_str(), r.to.as_str()), r))
        .collect();

    relations
        .iter()
        .filter(|r| r.from != r.to && is_known(&r.from) && is_known(&r.to))
        .enumerate()
        .filter_map(|(i, r)| {
            let reverse = by_pair.get(&(r.to.as_str(), r.from.as_str()));
            if reverse.is_some() && r.from > r.to {
                return None;
            }
            let label = r
                .fk_names
                .iter()
                .chain(reverse.iter().flat_map(|reverse| &reverse.fk_names))
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            Some(
                Edge::new(format!("fk-{i}"), r.from.as_str(), r.to.as_str()).with_content(FkEdge {
                    label,
                    bidirectional: reverse.is_some(),
                    ..FkEdge::default()
                }),
            )
        })
        .collect()
}

/// Sugiyama lays out each connected component from the origin, so components
/// (and FK-less tables) would pile up on top of each other. Arrange them in
/// rows instead, largest first. Without a layered layout every table is packed
/// on its own.
fn pack_components(flow: &mut ErFlow, keep_connected: bool) {
    let ids: Vec<String> = flow.nodes().map(|node| node.id.clone()).collect();
    let index: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for edge in flow.edges().iter().filter(|_| keep_connected) {
        if let (Some(&a), Some(&b)) = (
            index.get(edge.source.as_str()),
            index.get(edge.target.as_str()),
        ) {
            let (ra, rb) = (find_root(&mut parent, a), find_root(&mut parent, b));
            parent[ra] = rb;
        }
    }

    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..ids.len() {
        let root = find_root(&mut parent, i);
        components.entry(root).or_default().push(i);
    }
    let mut components: Vec<Vec<usize>> = components.into_values().collect();
    components.sort_by_key(|members| (std::cmp::Reverse(members.len()), members[0]));

    let bounds: Vec<rataflow::Rect> = components
        .iter()
        .map(|members| {
            members
                .iter()
                .map(|&i| {
                    flow.node(&ids[i])
                        .expect("node ids come from the flow")
                        .bounds()
                })
                .reduce(|a, b| a.union(&b))
                .expect("components are never empty")
        })
        .collect();

    // Aim for a roughly 2:1 (columns:rows) canvas, which looks square in cells.
    let area: f64 = bounds
        .iter()
        .map(|b| (b.width() + COMPONENT_GAP) * (b.height() + COMPONENT_GAP))
        .sum();
    let widest = bounds.iter().map(rataflow::Rect::width).fold(0.0, f64::max);
    let row_limit = widest.max((area * 2.0).sqrt());

    let mut positions = Vec::with_capacity(ids.len());
    let (mut x, mut y, mut row_height) = (0.0, 0.0, 0.0_f64);
    for (members, b) in components.iter().zip(&bounds) {
        if x > 0.0 && x + b.width() > row_limit {
            x = 0.0;
            y += row_height + COMPONENT_GAP;
            row_height = 0.0;
        }
        for &i in members {
            let node = flow.node(&ids[i]).expect("node ids come from the flow");
            positions.push((
                ids[i].clone(),
                Position::new(node.position.x - b.x() + x, node.position.y - b.y() + y),
            ));
        }
        x += b.width() + COMPONENT_COLUMN_GAP;
        row_height = row_height.max(b.height());
    }
    flow.set_node_positions(positions);
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// One table's turn-in line: where edges entering it from one side turn in.
struct TurnIn {
    /// The column side it lies in front of: whether it is the left one, and its x.
    column: (bool, i64),
    /// The rows it spans, from the lanes of its edges to the table's own row.
    top: f64,
    bottom: f64,
    /// How far the column side lies beyond the table's own side.
    inset: f64,
    /// Tracks that fit in front of the column side without reaching the
    /// column beyond.
    room: usize,
    edges: Vec<String>,
}

/// Gives each table a track for its turn-in line in front of the column side
/// it is entered from. Lines that would overlap get different tracks, keeping
/// edges into different tables apart while edges into one table still merge.
/// Once the gap in front of a column is full, its outermost track is shared
/// rather than reaching into the column beyond.
pub(super) fn assign_tracks(flow: &mut ErFlow) {
    let all_bounds: Vec<rataflow::Rect> = flow
        .nodes()
        .filter_map(|node| flow.node_bounds(&node.id))
        .collect();
    let mut turn_ins: BTreeMap<(bool, &str), TurnIn> = BTreeMap::new();
    for edge in flow.edges() {
        let (Some(source), Some(target)) = (
            flow.node_bounds(&edge.source),
            flow.node_bounds(&edge.target),
        ) else {
            continue;
        };
        let (source_side, target_side) = facing_sides(&source, &target);
        let enters_left = match target_side {
            HandlePosition::Left => true,
            HandlePosition::Right => false,
            HandlePosition::Top | HandlePosition::Bottom => continue,
        };
        let lane = departure_point(&source, source_side, target.center()).y;
        let turn_in = turn_ins
            .entry((enters_left, edge.target.as_str()))
            .or_insert_with(|| column_turn_in(&target, enters_left, &all_bounds));
        turn_in.top = turn_in.top.min(lane);
        turn_in.bottom = turn_in.bottom.max(lane);
        turn_in.edges.push(edge.id.clone());
    }

    let mut columns: BTreeMap<(bool, i64), Vec<TurnIn>> = BTreeMap::new();
    for turn_in in turn_ins.into_values() {
        columns.entry(turn_in.column).or_default().push(turn_in);
    }
    let mut extra_approaches = Vec::new();
    for mut turn_ins in columns.into_values() {
        turn_ins.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.bottom.total_cmp(&b.bottom)));
        // Lowest row each track is taken to.
        let mut track_bottoms: Vec<f64> = Vec::new();
        for turn_in in turn_ins {
            // A free row between two lines keeps them from reading as one.
            let free = track_bottoms
                .iter()
                .position(|&bottom| bottom + 1.0 < turn_in.top);
            let track = free.unwrap_or(track_bottoms.len()).min(turn_in.room - 1);
            if track == track_bottoms.len() {
                track_bottoms.push(turn_in.bottom);
            } else {
                track_bottoms[track] = track_bottoms[track].max(turn_in.bottom);
            }
            let extra_approach = (track as f64).mul_add(TRACK_SPACING, turn_in.inset);
            extra_approaches.extend(turn_in.edges.into_iter().map(|id| (id, extra_approach)));
        }
    }
    for (edge_id, extra_approach) in extra_approaches {
        if let Some(content) = flow.edge_content_mut(&edge_id) {
            content.extra_approach = extra_approach;
        }
    }
}

/// An empty turn-in line in front of the column side `target` is entered from.
/// Tables in a column share their left side, so the right side of a column is
/// that of its widest table.
fn column_turn_in(target: &rataflow::Rect, enters_left: bool, all: &[rataflow::Rect]) -> TurnIn {
    let in_column = all.iter().filter(|b| (b.x() - target.x()).abs() < 0.5);
    let (side, gap) = if enters_left {
        let side = target.x();
        let before = all
            .iter()
            .map(rataflow::Rect::right)
            .filter(|&right| right <= side);
        (side, side - before.fold(f64::NEG_INFINITY, f64::max))
    } else {
        let side = in_column
            .map(rataflow::Rect::right)
            .fold(target.right(), f64::max);
        let after = all.iter().map(rataflow::Rect::x).filter(|&x| x >= side);
        (side, after.fold(f64::INFINITY, f64::min) - side)
    };
    // Past the stem of an edge leaving the column beyond, with a cell to turn in.
    let spare = gap - EDGE_STEM - 1.0 - EDGE_APPROACH;
    let room = if spare.is_finite() {
        (spare / TRACK_SPACING).floor().max(0.0) as usize + 1
    } else {
        usize::MAX
    };
    let row = target.center().y;
    TurnIn {
        column: (enters_left, side.round() as i64),
        top: row,
        bottom: row,
        inset: if enters_left {
            0.0
        } else {
            side - target.right()
        },
        room,
        edges: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::overlays::er_diagram::test_support::{
        isolated_tables, sample_tables, table,
    };

    fn sample_edges() -> Vec<Edge<FkEdge>> {
        relation_edges(&collect_relations(&sample_tables()), |_| true)
    }

    #[test]
    fn parallel_fks_share_one_edge() {
        let edges = sample_edges();

        let edge = edges
            .iter()
            .find(|e| e.connects_directed("public.posts", "public.users"))
            .unwrap();
        assert_eq!(edge.content.label, "fk_author, fk_editor");
        assert!(!edge.content.bidirectional);
    }

    #[test]
    fn mutual_references_become_one_edge_with_both_names() {
        let edges = sample_edges();

        let between_a_b: Vec<_> = edges
            .iter()
            .filter(|e| e.connects("public.a", "public.b"))
            .collect();
        assert_eq!(between_a_b.len(), 1);
        assert_eq!(between_a_b[0].content.label, "fk_ab, fk_ba");
        assert!(between_a_b[0].content.bidirectional);
    }

    #[test]
    fn self_references_and_unknown_targets_are_not_edges() {
        let relations = collect_relations(&[
            table("public.comments", &[("fk_parent", "public.comments")]),
            table("public.posts", &[("fk_missing", "public.missing")]),
        ]);

        let edges = relation_edges(&relations, |id| id != "public.missing");

        assert!(edges.is_empty());
    }

    #[test]
    fn self_reference_is_listed_inside_the_table_node() {
        let tables = sample_tables();

        let flow = table_graph(&tables, &collect_relations(&tables)).unwrap();

        let content = &flow.node("public.comments").unwrap().content;
        assert_eq!(content.name, "comments");
        assert_eq!(content.self_reference_fks, ["fk_parent"]);
    }

    #[test]
    fn schema_is_shown_only_when_schemas_are_mixed() {
        let mut tables = sample_tables();
        let relations = collect_relations(&tables);
        let single = table_graph(&tables, &relations).unwrap();
        tables.push(table("audit.logs", &[]));

        let mixed = table_graph(&tables, &relations).unwrap();

        let schema = |flow: &ErFlow| flow.node("public.users").unwrap().content.schema.clone();
        assert_eq!(schema(&single), None);
        assert_eq!(schema(&mixed).as_deref(), Some("public"));
    }

    fn positioned(tables: &[ErTableInfo], positions: &[(&str, f64, f64)]) -> ErFlow {
        let mut flow = table_graph(tables, &collect_relations(tables)).unwrap();
        flow.set_node_positions(
            positions
                .iter()
                .map(|&(id, x, y)| (id, Position::new(x, y))),
        );
        assign_tracks(&mut flow);
        flow
    }

    fn extra_approach(flow: &ErFlow, from: &str, to: &str) -> f64 {
        flow.edges()
            .iter()
            .find(|e| e.connects_directed(from, to))
            .unwrap()
            .content
            .extra_approach
    }

    #[test]
    fn overlapping_turn_ins_get_their_own_tracks() {
        let tables = [
            table(
                "public.a",
                &[("fk_a_x", "public.x"), ("fk_a_y", "public.y")],
            ),
            table("public.b", &[("fk_b_y", "public.y")]),
            table("public.x", &[]),
            table("public.y", &[]),
        ];

        let flow = positioned(
            &tables,
            &[
                ("public.a", 0.0, 0.0),
                ("public.b", 0.0, 10.0),
                ("public.x", 40.0, 0.0),
                ("public.y", 40.0, 10.0),
            ],
        );

        assert_eq!(extra_approach(&flow, "public.a", "public.x"), 0.0);
        assert_eq!(extra_approach(&flow, "public.a", "public.y"), TRACK_SPACING);
        assert_eq!(extra_approach(&flow, "public.b", "public.y"), TRACK_SPACING);
    }

    #[test]
    fn turn_ins_far_apart_share_a_track() {
        let tables = [
            table("public.a", &[("fk_a_x", "public.x")]),
            table("public.b", &[("fk_b_y", "public.y")]),
            table("public.x", &[]),
            table("public.y", &[]),
        ];

        let flow = positioned(
            &tables,
            &[
                ("public.a", 0.0, 0.0),
                ("public.b", 0.0, 30.0),
                ("public.x", 40.0, 0.0),
                ("public.y", 40.0, 30.0),
            ],
        );

        assert_eq!(extra_approach(&flow, "public.a", "public.x"), 0.0);
        assert_eq!(extra_approach(&flow, "public.b", "public.y"), 0.0);
    }

    #[test]
    fn turn_in_from_the_right_clears_wider_tables_in_the_column() {
        let tables = [
            table("public.s", &[("fk_s_t", "public.t")]),
            table("public.t", &[]),
            table("public.a_much_wider_table", &[]),
        ];

        let flow = positioned(
            &tables,
            &[
                ("public.t", 0.0, 0.0),
                ("public.a_much_wider_table", 0.0, 5.0),
                ("public.s", 80.0, 10.0),
            ],
        );

        let t = flow.node_bounds("public.t").unwrap();
        let wide = flow.node_bounds("public.a_much_wider_table").unwrap();
        let turn_in_x = t.right() + EDGE_APPROACH + extra_approach(&flow, "public.s", "public.t");
        assert!(turn_in_x > wide.right(), "{turn_in_x} vs {}", wide.right());
    }

    #[test]
    fn turn_ins_stay_in_front_of_the_column_beyond() {
        let targets = [
            "public.t1",
            "public.t2",
            "public.t3",
            "public.t4",
            "public.t5",
        ];
        let fks: Vec<(String, &str)> = targets.iter().map(|t| (format!("fk_{t}"), *t)).collect();
        let fks: Vec<(&str, &str)> = fks.iter().map(|(name, t)| (name.as_str(), *t)).collect();
        let mut tables = vec![table("public.s", &fks)];
        tables.extend(targets.iter().map(|t| table(t, &[])));
        // Room for three tracks between `s` (five cells wide) and the targets.
        let column_x = 13.0;
        let mut positions = vec![("public.s", 0.0, 0.0)];
        positions.extend(
            targets
                .iter()
                .zip(0..)
                .map(|(t, i)| (*t, column_x, f64::from(i) * 4.0)),
        );

        let flow = positioned(&tables, &positions);

        let s = flow.node_bounds("public.s").unwrap();
        for target in targets {
            let turn_in_x = column_x - EDGE_APPROACH - extra_approach(&flow, "public.s", target);
            assert!(turn_in_x > s.right() + EDGE_STEM, "{target}: {turn_in_x}");
        }
    }

    fn assert_no_overlap(tables: &[ErTableInfo]) {
        let mut flow = table_graph(tables, &collect_relations(tables)).unwrap();
        let layout = layout_tables(tables);
        flow.set_node_positions(
            layout
                .positions
                .iter()
                .map(|(id, &(x, y))| (id, Position::new(x, y))),
        );

        let bounds: Vec<_> = flow.nodes().map(Node::bounds).collect();
        assert_eq!(bounds.len(), tables.len());
        for (i, a) in bounds.iter().enumerate() {
            for b in &bounds[i + 1..] {
                assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn layout_places_every_table_without_overlap() {
        assert_no_overlap(&sample_tables());
    }

    #[test]
    fn grid_fallback_places_tables_without_overlap() {
        let mut tables = isolated_tables(LAYERED_LAYOUT_MAX_TABLES + 1);
        tables[1] = table("public.t1", &[("fk", "public.t0")]);

        assert_no_overlap(&tables);
    }
}
