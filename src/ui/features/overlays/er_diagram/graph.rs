use std::collections::{BTreeMap, HashMap, HashSet};

use rataflow::{Edge, Flow, Node, Position, Sugiyama};
use unicode_width::UnicodeWidthStr;

use super::content::{EDGE_APPROACH, EDGE_STEM, FkEdge, SELF_REFERENCE_MARK, TableNode};
use crate::app::model::er_state::ErLayout;
use crate::domain::ErTableInfo;

pub(super) type ErFlow = Flow<TableNode, FkEdge>;

const NODE_SPACING: f64 = 2.0;
const MIN_RANK_SPACING: usize = 8;
const MAX_RANK_SPACING: usize = 36;
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
        // Room for the label on an edge's lane between its stem and approach.
        let rank_spacing = flow
            .edges()
            .iter()
            .map(|edge| edge.content.label.width())
            .max()
            .map_or(MIN_RANK_SPACING, |width| {
                width + (EDGE_STEM + EDGE_APPROACH) as usize + 1
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
