use std::collections::{BTreeMap, HashMap, HashSet};

use rataflow::{
    ControlsAction, Edge, EdgeMarker, EdgeStyle, EventResponse, FitViewOptions, FloatingEdge, Flow,
    FlowAction, Node, Palette, Position, Sugiyama, TextContent, Theme,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::model::app_state::AppState;
use crate::app::model::er_state::{ErDiagram, ErLayout, ErViewerCommand};
use crate::app::update::input::keybindings::er_diagram;
use crate::domain::ErTableInfo;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

type ErFlow = Flow<TextContent, FloatingEdge>;

const NODE_SPACING: f64 = 2.0;
const MIN_RANK_SPACING: usize = 8;
const MAX_RANK_SPACING: usize = 36;
const COMPONENT_GAP: f64 = 4.0;
const COMPONENT_COLUMN_GAP: f64 = COMPONENT_GAP * 2.0;
const MIN_ZOOM: f64 = 0.2;
const FIT_PADDING: f64 = 1.0;
const SELF_REFERENCE_MARK: &str = "↻";

/// FK constraints between one ordered pair of tables.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Relation {
    from: String,
    to: String,
    fk_names: Vec<String>,
}

/// Renderer-owned state of the ER diagram viewer.
///
/// The app layer only holds the diagram data; layout, viewport and selection
/// live in the rataflow `Flow`, which is rebuilt whenever a new diagram arrives.
#[derive(Default)]
pub struct ErDiagramView {
    diagram_id: Option<u64>,
    flow: Option<ErFlow>,
    relations: Vec<Relation>,
    build_error: Option<String>,
}

impl ErDiagramView {
    pub fn sync(&mut self, diagram: Option<&ErDiagram>) {
        let Some(diagram) = diagram else {
            *self = Self::default();
            return;
        };
        if self.diagram_id == Some(diagram.id) {
            return;
        }

        let relations = collect_relations(&diagram.tables);
        let (flow, build_error) = match build_flow(&diagram.tables, &relations, &diagram.layout) {
            Ok(flow) => (Some(flow), None),
            Err(e) => (None, Some(e.to_string())),
        };
        *self = Self {
            diagram_id: Some(diagram.id),
            flow,
            relations,
            build_error,
        };
    }

    pub fn apply(&mut self, command: ErViewerCommand) {
        let Some(flow) = self.flow.as_mut() else {
            return;
        };
        let _ = match command {
            ErViewerCommand::SelectLeft => flow.apply(FlowAction::SelectLeft),
            ErViewerCommand::SelectDown => flow.apply(FlowAction::SelectDown),
            ErViewerCommand::SelectUp => flow.apply(FlowAction::SelectUp),
            ErViewerCommand::SelectRight => flow.apply(FlowAction::SelectRight),
            ErViewerCommand::SelectNext => flow.apply(FlowAction::SelectNext),
            ErViewerCommand::SelectPrev => flow.apply(FlowAction::SelectPrev),
            ErViewerCommand::PanLeft => flow.apply(FlowAction::PanLeft),
            ErViewerCommand::PanDown => flow.apply(FlowAction::PanDown),
            ErViewerCommand::PanUp => flow.apply(FlowAction::PanUp),
            ErViewerCommand::PanRight => flow.apply(FlowAction::PanRight),
            ErViewerCommand::CenterOnSelected => flow.apply(FlowAction::CenterOnSelected),
            ErViewerCommand::ZoomIn => flow.apply_controls_action(ControlsAction::ZoomIn),
            ErViewerCommand::ZoomOut => flow.apply_controls_action(ControlsAction::ZoomOut),
            ErViewerCommand::ResetZoom => flow.apply_controls_action(ControlsAction::ResetZoom),
            ErViewerCommand::FitView => {
                flow.request_fit_view_with_options(FitViewOptions {
                    padding: FIT_PADDING,
                    max_zoom: Some(1.0),
                    ..FitViewOptions::default()
                });
                EventResponse::Handled
            }
        };
    }

    fn selected_table(&self) -> Option<String> {
        self.flow.as_ref()?.first_selected_node_id()
    }
}

pub struct ErDiagramViewer;

impl ErDiagramViewer {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        view: &mut ErDiagramView,
        theme: &ThemePalette,
    ) {
        view.sync(state.er_diagram.as_ref());
        let Some(diagram) = state.er_diagram.as_ref() else {
            return;
        };

        let title = format!(
            " ER Diagram ({}/{} tables) ",
            diagram.tables.len(),
            diagram.total_tables
        );
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(96),
            Constraint::Percentage(92),
            &title,
            FooterHintBar::new([
                er_diagram::SELECT.as_hint(),
                er_diagram::PAN.as_hint(),
                er_diagram::ZOOM.as_hint(),
                er_diagram::FIT.as_hint(),
                er_diagram::HELP.as_hint(),
                er_diagram::ESC_CLOSE.as_hint(),
            ]),
            theme,
        );
        let [canvas_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        let selected = view.selected_table();
        let status = status_line(selected.as_deref(), &view.relations, diagram, theme);
        frame.render_widget(Paragraph::new(status), status_area);

        let Some(flow) = view.flow.as_mut() else {
            let message = view.build_error.as_deref().unwrap_or("Nothing to draw");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!("Failed to lay out ER diagram: {message}"),
                    Style::default().fg(theme.semantic.status.error),
                )),
                canvas_area,
            );
            return;
        };
        flow.theme = Theme::Custom(flow_palette(theme));
        apply_edge_styles(flow, &view.relations, selected.as_deref(), theme);
        frame.render_widget(&mut *flow, canvas_area);
        // Fit and center can leave a half-cell pan offset, which makes box borders
        // and edge endpoints round to different rows. Snap to whole cells.
        let (x, y) = (flow.viewport.x.round(), flow.viewport.y.round());
        if (x, y) != (flow.viewport.x, flow.viewport.y) {
            flow.viewport.x = x;
            flow.viewport.y = y;
            frame.render_widget(Clear, canvas_area);
            frame.render_widget(flow, canvas_area);
        }
    }
}

fn collect_relations(tables: &[ErTableInfo]) -> Vec<Relation> {
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

/// Computes table positions with rataflow's Sugiyama layout. Runs on a
/// blocking task because it is slow for large schemas.
pub fn layout_tables(tables: &[ErTableInfo], seed_tables: &[String]) -> ErLayout {
    let relations = collect_relations(tables);
    let Ok(mut flow) = graph_flow(tables, &relations) else {
        return ErLayout::default();
    };

    let rank_spacing = flow
        .edges()
        .iter()
        .filter_map(|edge| edge.label.as_deref())
        .map(UnicodeWidthStr::width)
        .max()
        .map_or(MIN_RANK_SPACING, |width| width + 4)
        .clamp(MIN_RANK_SPACING, MAX_RANK_SPACING);
    // Left-to-right keeps FK labels on horizontal segments, where they read best.
    flow.apply_layout(
        Sugiyama::horizontal()
            .with_rank_spacing(rank_spacing as f64)
            .with_node_spacing(NODE_SPACING),
    );
    let main_component = pack_components(&mut flow);
    let seeds: Vec<String> = seed_tables
        .iter()
        .filter(|id| flow.node(id).is_some())
        .cloned()
        .collect();
    let focus_tables = if seeds.is_empty() {
        main_component
    } else {
        seeds
    };

    ErLayout {
        positions: flow
            .nodes()
            .map(|node| {
                let position = (node.position.x.round(), node.position.y.round());
                (node.id.clone(), position)
            })
            .collect(),
        focus_tables,
    }
}

fn graph_flow(tables: &[ErTableInfo], relations: &[Relation]) -> Result<ErFlow, rataflow::Error> {
    let mut seen = HashSet::new();
    let nodes: Vec<Node<TextContent>> = tables
        .iter()
        .filter(|table| seen.insert(table.qualified_name.as_str()))
        .map(|table| table_node(table, relations))
        .collect();
    let edges = relation_edges(relations, |id| seen.contains(id));
    ErFlow::with_graph(nodes, edges)
}

fn build_flow(
    tables: &[ErTableInfo],
    relations: &[Relation],
    layout: &ErLayout,
) -> Result<ErFlow, rataflow::Error> {
    let mut flow = graph_flow(tables, relations)?.with_min_zoom(MIN_ZOOM);
    flow.set_node_positions(
        layout
            .positions
            .iter()
            .map(|(id, &(x, y))| (id, Position::new(x, y))),
    );

    let ids: Vec<String> = flow.nodes().map(|node| node.id.clone()).collect();
    for id in &ids {
        flow.set_handles_hidden(id, true);
    }
    // A single focused table is the picked table of a partial diagram.
    if let [only] = layout.focus_tables.as_slice() {
        flow.select_node(only);
    }
    // Start at full size so names stay legible; `f` zooms out to show everything.
    flow.request_fit_view_with_options(FitViewOptions {
        padding: FIT_PADDING,
        min_zoom: Some(1.0),
        max_zoom: Some(1.0),
        nodes: layout.focus_tables.clone(),
        ..FitViewOptions::default()
    });
    Ok(flow)
}

fn table_node(table: &ErTableInfo, relations: &[Relation]) -> Node<TextContent> {
    let mut lines = vec![table.name.clone()];
    lines.extend(
        relations
            .iter()
            .filter(|r| r.from == table.qualified_name && r.to == table.qualified_name)
            .flat_map(|r| &r.fk_names)
            .map(|name| format!("{SELF_REFERENCE_MARK} {name}")),
    );
    let title = format!(" {} ", table.schema);
    let width = lines
        .iter()
        .map(|line| line.width())
        .chain([title.width()])
        .max()
        .unwrap_or(0)
        + 4;
    let height = lines.len() + 2;

    Node::new(
        table.qualified_name.clone(),
        (0.0, 0.0),
        (width as f64, height as f64),
        TextContent::new(lines.join("\n")).with_title(title),
    )
}

/// One edge per table pair: parallel FKs share a label, and mutual references
/// become a single double-headed edge since both directions would overlap.
fn relation_edges(
    relations: &[Relation],
    is_known: impl Fn(&str) -> bool,
) -> Vec<Edge<FloatingEdge>> {
    let by_pair: HashMap<(&str, &str), &Relation> = relations
        .iter()
        .map(|r| ((r.from.as_str(), r.to.as_str()), r))
        .collect();

    relations
        .iter()
        .filter(|r| r.from != r.to && is_known(&r.from) && is_known(&r.to))
        .filter_map(|r| {
            let reverse = by_pair.get(&(r.to.as_str(), r.from.as_str()));
            if reverse.is_some() && r.from > r.to {
                return None;
            }
            let mut names = r.fk_names.clone();
            if let Some(reverse) = reverse {
                names.extend(reverse.fk_names.iter().cloned());
            }
            Some(
                Edge::new(edge_id(&r.from, &r.to), r.from.as_str(), r.to.as_str())
                    .with_label(names.join(", "))
                    .with_content(FloatingEdge::stepped()),
            )
        })
        .collect()
}

fn edge_id(from: &str, to: &str) -> String {
    format!("{from}->{to}")
}

/// Sugiyama lays out each connected component from the origin, so components
/// (and FK-less tables) would pile up on top of each other. Arrange them in
/// rows instead, largest first, and return the ids of the largest component.
fn pack_components(flow: &mut ErFlow) -> Vec<String> {
    let ids: Vec<String> = flow.nodes().map(|node| node.id.clone()).collect();
    let index: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for edge in flow.edges() {
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

    let bounds: Vec<(Position, Position)> = components
        .iter()
        .map(|members| {
            members.iter().fold(
                (
                    Position::new(f64::MAX, f64::MAX),
                    Position::new(f64::MIN, f64::MIN),
                ),
                |(min, max), &i| {
                    let node = flow.node(&ids[i]).expect("node ids come from the flow");
                    (
                        Position::new(min.x.min(node.position.x), min.y.min(node.position.y)),
                        Position::new(
                            max.x.max(node.position.x + node.width),
                            max.y.max(node.position.y + node.height),
                        ),
                    )
                },
            )
        })
        .collect();

    // Aim for a roughly 2:1 (columns:rows) canvas, which looks square in cells.
    let area: f64 = bounds
        .iter()
        .map(|(min, max)| (max.x - min.x + COMPONENT_GAP) * (max.y - min.y + COMPONENT_GAP))
        .sum();
    let widest = bounds
        .iter()
        .map(|(min, max)| max.x - min.x)
        .fold(0.0, f64::max);
    let row_limit = widest.max((area * 2.0).sqrt());

    let mut positions = Vec::with_capacity(ids.len());
    let (mut x, mut y, mut row_height) = (0.0, 0.0, 0.0_f64);
    for (members, (min, max)) in components.iter().zip(&bounds) {
        let (width, height) = (max.x - min.x, max.y - min.y);
        if x > 0.0 && x + width > row_limit {
            x = 0.0;
            y += row_height + COMPONENT_GAP;
            row_height = 0.0;
        }
        for &i in members {
            let node = flow.node(&ids[i]).expect("node ids come from the flow");
            positions.push((
                ids[i].clone(),
                Position::new(node.position.x - min.x + x, node.position.y - min.y + y),
            ));
        }
        x += width + COMPONENT_COLUMN_GAP;
        row_height = row_height.max(height);
    }
    flow.set_node_positions(positions);

    components
        .first()
        .map(|members| members.iter().map(|&i| ids[i].clone()).collect())
        .unwrap_or_default()
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn flow_palette(theme: &ThemePalette) -> Palette {
    Palette {
        canvas_bg: ratatui::style::Color::Reset,
        surface: ratatui::style::Color::Reset,
        muted: theme.semantic.surface.unfocus_border,
        subtle: theme.semantic.text.dim,
        accent: theme.semantic.surface.focus_border,
        text: theme.semantic.text.primary,
        success: theme.semantic.status.success,
        error: theme.semantic.status.error,
    }
}

/// Edges touching the selected table are drawn in the accent color and carry
/// their FK names; the rest stay unlabeled so dense diagrams remain readable
/// (labels do not shrink when zoomed out).
fn apply_edge_styles(
    flow: &mut ErFlow,
    relations: &[Relation],
    selected: Option<&str>,
    theme: &ThemePalette,
) {
    let fk_names: HashMap<(&str, &str), &[String]> = relations
        .iter()
        .map(|r| ((r.from.as_str(), r.to.as_str()), r.fk_names.as_slice()))
        .collect();
    let edges: Vec<(String, bool, bool, Option<String>)> = flow
        .edges()
        .iter()
        .map(|edge| {
            let highlighted = selected.is_some_and(|id| edge.source == id || edge.target == id);
            let forward = fk_names.get(&(edge.source.as_str(), edge.target.as_str()));
            let reverse = fk_names.get(&(edge.target.as_str(), edge.source.as_str()));
            let label = highlighted.then(|| {
                forward
                    .into_iter()
                    .chain(reverse)
                    .flat_map(|names| names.iter())
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            (edge.id.clone(), highlighted, reverse.is_some(), label)
        })
        .collect();
    for (id, highlighted, bidirectional, label_text) in edges {
        let (stroke, label) = if highlighted {
            let accent = Style::default().fg(theme.semantic.surface.focus_border);
            (accent, accent.add_modifier(Modifier::BOLD))
        } else {
            (
                Style::default().fg(theme.semantic.surface.unfocus_border),
                Style::default().fg(theme.semantic.text.secondary),
            )
        };
        let mut style = EdgeStyle::default()
            .with_stroke_style(stroke)
            .with_label_style(label);
        if bidirectional {
            style = style.with_marker_start(EdgeMarker::ArrowClosed);
        }
        if let Some(content) = flow.edge_content_mut(&id) {
            content.style = Some(style);
        }
        flow.set_edge_label(&id, label_text);
    }
}

fn status_line(
    selected: Option<&str>,
    relations: &[Relation],
    diagram: &ErDiagram,
    theme: &ThemePalette,
) -> Line<'static> {
    let muted = Style::default().fg(theme.semantic.text.muted);
    let Some(selected) = selected else {
        let fk_count: usize = relations.iter().map(|r| r.fk_names.len()).sum();
        return Line::from(Span::styled(
            format!(
                " {} tables · {fk_count} foreign keys — press {} to select a table and show its foreign keys",
                diagram.tables.len(),
                er_diagram::SELECT.key_short,
            ),
            muted,
        ));
    };

    let mut spans = vec![Span::styled(
        format!(" {selected}"),
        Style::default()
            .fg(theme.semantic.surface.focus_border)
            .add_modifier(Modifier::BOLD),
    )];
    let outgoing = relations
        .iter()
        .filter(|r| r.from == selected && r.to != selected)
        .map(|r| ("→", &r.to, &r.fk_names));
    let incoming = relations
        .iter()
        .filter(|r| r.to == selected && r.from != selected)
        .map(|r| ("←", &r.from, &r.fk_names));
    for (arrow, table, fk_names) in outgoing.chain(incoming) {
        spans.push(Span::styled(format!("  {arrow} "), muted));
        spans.push(Span::styled(
            table.clone(),
            Style::default().fg(theme.semantic.text.primary),
        ));
        spans.push(Span::styled(format!(" ({})", fk_names.join(", ")), muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::theme_id::ThemeId;
    use crate::domain::er::ErFkInfo;
    use crate::theme::palette_for;

    fn table(qualified: &str, fks: &[(&str, &str)]) -> ErTableInfo {
        let (schema, name) = qualified.split_once('.').unwrap();
        ErTableInfo {
            qualified_name: qualified.to_string(),
            name: name.to_string(),
            schema: schema.to_string(),
            foreign_keys: fks
                .iter()
                .map(|(fk, to)| ErFkInfo {
                    name: (*fk).to_string(),
                    from_qualified: qualified.to_string(),
                    to_qualified: (*to).to_string(),
                })
                .collect(),
        }
    }

    fn diagram(id: u64, tables: Vec<ErTableInfo>) -> ErDiagram {
        let total_tables = tables.len();
        let layout = layout_tables(&tables, &[]);
        ErDiagram {
            id,
            tables,
            total_tables,
            layout,
        }
    }

    fn sample_tables() -> Vec<ErTableInfo> {
        vec![
            table("public.users", &[]),
            table(
                "public.posts",
                &[("fk_author", "public.users"), ("fk_editor", "public.users")],
            ),
            table(
                "public.comments",
                &[
                    ("fk_post", "public.posts"),
                    ("fk_parent", "public.comments"),
                ],
            ),
            table("public.a", &[("fk_ab", "public.b")]),
            table("public.b", &[("fk_ba", "public.a")]),
            table("public.orphan", &[]),
        ]
    }

    #[test]
    fn parallel_fks_share_one_edge() {
        let relations = collect_relations(&sample_tables());

        let edges = relation_edges(&relations, |_| true);

        let edge = edges
            .iter()
            .find(|e| e.id == edge_id("public.posts", "public.users"))
            .unwrap();
        assert_eq!(edge.label.as_deref(), Some("fk_author, fk_editor"));
    }

    #[test]
    fn mutual_references_become_one_edge_with_both_names() {
        let relations = collect_relations(&sample_tables());

        let edges = relation_edges(&relations, |_| true);

        let between_a_b: Vec<_> = edges
            .iter()
            .filter(|e| e.connects("public.a", "public.b"))
            .collect();
        assert_eq!(between_a_b.len(), 1);
        assert_eq!(between_a_b[0].label.as_deref(), Some("fk_ab, fk_ba"));
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
        let relations = collect_relations(&tables);

        let node = table_node(&tables[2], &relations);

        assert_eq!(node.content.label(), "comments");
        assert_eq!(node.content.text.lines.len(), 2);
        assert_eq!(node.content.text.lines[1].to_string(), "↻ fk_parent");
    }

    #[test]
    fn layout_places_every_table_without_overlap() {
        let tables = sample_tables();
        let relations = collect_relations(&tables);
        let layout = layout_tables(&tables, &[]);

        let flow = build_flow(&tables, &relations, &layout).unwrap();

        let bounds: Vec<_> = flow.nodes().map(Node::bounds).collect();
        assert_eq!(bounds.len(), tables.len());
        for (i, a) in bounds.iter().enumerate() {
            for b in &bounds[i + 1..] {
                let overlaps = a.x() < b.x() + b.width()
                    && b.x() < a.x() + a.width()
                    && a.y() < b.y() + b.height()
                    && b.y() < a.y() + a.height();
                assert!(!overlaps, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn select_command_selects_a_table() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));

        view.apply(ErViewerCommand::SelectNext);

        assert!(view.selected_table().is_some());
    }

    #[test]
    fn new_diagram_resets_selection() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));
        view.apply(ErViewerCommand::SelectNext);

        view.sync(Some(&diagram(2, sample_tables())));

        assert!(view.selected_table().is_none());
    }

    #[test]
    fn same_diagram_keeps_selection() {
        let mut view = ErDiagramView::default();
        let diagram = diagram(1, sample_tables());
        view.sync(Some(&diagram));
        view.apply(ErViewerCommand::SelectNext);

        view.sync(Some(&diagram));

        assert!(view.selected_table().is_some());
    }

    #[test]
    fn single_seed_table_starts_selected() {
        let tables = sample_tables();
        let layout = layout_tables(&tables, &["public.posts".to_string()]);
        let total_tables = tables.len();
        let mut view = ErDiagramView::default();

        view.sync(Some(&ErDiagram {
            id: 1,
            tables,
            total_tables,
            layout,
        }));

        assert_eq!(view.selected_table().as_deref(), Some("public.posts"));
    }

    #[test]
    fn full_diagram_focuses_largest_component() {
        let layout = layout_tables(&sample_tables(), &[]);

        let mut focus = layout.focus_tables;
        focus.sort();
        assert_eq!(focus, ["public.comments", "public.posts", "public.users"]);
    }

    #[test]
    fn closed_diagram_drops_flow() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));

        view.sync(None);

        assert!(view.flow.is_none());
        assert!(view.diagram_id.is_none());
    }

    #[test]
    fn status_line_lists_outgoing_and_incoming_fks() {
        let tables = sample_tables();
        let relations = collect_relations(&tables);
        let theme = palette_for(ThemeId::Default);

        let line = status_line(Some("public.posts"), &relations, &diagram(1, tables), theme);

        assert_eq!(
            line.to_string(),
            " public.posts  → public.users (fk_author, fk_editor)  ← public.comments (fk_post)"
        );
    }
}
