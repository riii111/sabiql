use std::collections::{BTreeMap, HashMap, HashSet};

use rataflow::{
    ControlsAction, Edge, EdgeMarker, EdgeStyle, EventResponse, FitViewOptions, FloatingAttachment,
    FloatingEdge, Flow, FlowAction, Node, NodeContent, NodeRenderContext, Palette, Position,
    Sugiyama, Theme,
};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::model::app_state::AppState;
use crate::app::model::er_state::{ErDiagram, ErLayout, ErViewerCommand};
use crate::app::update::input::keybindings::er_diagram;
use crate::domain::ErTableInfo;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::primitives::utils::text_utils::wrapped_line_count;
use crate::theme::ThemePalette;

type ErFlow = Flow<TableNode, FloatingEdge>;

const NODE_SPACING: f64 = 2.0;
const MIN_RANK_SPACING: usize = 8;
const MAX_RANK_SPACING: usize = 36;
const COMPONENT_GAP: f64 = 4.0;
const COMPONENT_COLUMN_GAP: f64 = COMPONENT_GAP * 2.0;
const MIN_ZOOM: f64 = 0.2;
// rataflow's Sugiyama layout grows superlinearly (seconds past ~1000 tables);
// beyond this, tables are packed in a grid instead.
const LAYERED_LAYOUT_MAX_TABLES: usize = 800;
const FIT_PADDING: f64 = 1.0;
const MAX_STATUS_LINES: u16 = 3;
const SELF_REFERENCE_MARK: &str = "↻";

/// A table box. When zoomed out too far for a box with text inside, the name
/// moves onto the border so tables stay identifiable.
#[derive(Debug)]
struct TableNode {
    name: String,
    /// Shown only when the diagram spans several schemas.
    schema: Option<String>,
    self_reference_fks: Vec<String>,
}

impl NodeContent for TableNode {
    fn render(&self, ctx: &NodeRenderContext, buf: &mut Buffer) {
        let palette = ctx.theme.palette();
        let border = if ctx.selected {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.muted)
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border)
            .bg(palette.surface);
        let text = Style::default().fg(palette.text);

        if ctx.area.height < 3 {
            block
                .title(Span::styled(self.name.as_str(), text))
                .render(ctx.area, buf);
            return;
        }

        let block = match &self.schema {
            Some(schema) => block.title(format!(" {schema} ")),
            None => block,
        };
        let inner = block.inner(ctx.area);
        block.render(ctx.area, buf);

        let mut lines = vec![Line::styled(self.name.as_str(), text)];
        lines.extend(self.self_reference_fks.iter().map(|name| {
            Line::styled(
                format!("{SELF_REFERENCE_MARK} {name}"),
                Style::default().fg(palette.muted),
            )
        }));
        let top_padding = inner.height.saturating_sub(lines.len() as u16) / 2;
        let text_area = Rect {
            y: inner.y + top_padding,
            height: inner.height - top_padding,
            ..inner
        };
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .render(text_area, buf);
    }
}

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
    show_all_fk_names: bool,
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
            show_all_fk_names: false,
        };
    }

    pub fn apply(&mut self, command: ErViewerCommand) {
        if command == ErViewerCommand::ToggleFkNames {
            self.show_all_fk_names = !self.show_all_fk_names;
            return;
        }
        let Some(flow) = self.flow.as_mut() else {
            return;
        };
        // Fit may go below the manual zoom floor; zooming out from there must not
        // jump back up to it.
        flow.min_zoom = flow.min_zoom.min(flow.viewport.zoom);
        let directional = matches!(
            command,
            ErViewerCommand::SelectLeft
                | ErViewerCommand::SelectDown
                | ErViewerCommand::SelectUp
                | ErViewerCommand::SelectRight
        );
        if directional && flow.first_selected_node_id().is_none() {
            // rataflow would jump to the first table; start where the user is looking.
            if let Some(id) = table_nearest_view_center(flow) {
                flow.select_node(&id);
            }
            return;
        }
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
            ErViewerCommand::PanHalfPageDown => pan_by_page(flow, 0.5),
            ErViewerCommand::PanHalfPageUp => pan_by_page(flow, -0.5),
            ErViewerCommand::PanPageDown => pan_by_page(flow, 1.0),
            ErViewerCommand::PanPageUp => pan_by_page(flow, -1.0),
            ErViewerCommand::ClearSelection => flow.apply(FlowAction::ClearSelection),
            ErViewerCommand::CenterOnSelected => flow.apply(FlowAction::CenterOnSelected),
            ErViewerCommand::ZoomIn => flow.apply_controls_action(ControlsAction::ZoomIn),
            ErViewerCommand::ZoomOut => flow.apply_controls_action(ControlsAction::ZoomOut),
            ErViewerCommand::ResetZoom => flow.apply_controls_action(ControlsAction::ResetZoom),
            ErViewerCommand::ToggleFkNames => EventResponse::NotHandled,
            ErViewerCommand::FitView => {
                flow.request_fit_view_with_options(FitViewOptions {
                    padding: FIT_PADDING,
                    // Whatever it takes to show everything.
                    min_zoom: Some(0.0),
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

fn pan_by_page(flow: &mut ErFlow, pages: f64) -> EventResponse {
    let dy = flow.canvas_size().height * pages;
    flow.apply(FlowAction::Pan { dx: 0.0, dy })
}

pub struct ErDiagramViewer;

impl ErDiagramViewer {
    /// Returns whether a table is selected, which decides what Esc does.
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        view: &mut ErDiagramView,
        theme: &ThemePalette,
    ) -> bool {
        let Some(diagram) = state.er_diagram.as_ref() else {
            return false;
        };
        let selected = view.selected_table();
        let esc_hint = if selected.is_some() {
            (er_diagram::ESC.key_short, "Deselect")
        } else {
            er_diagram::ESC.as_hint()
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
                er_diagram::FK_NAMES.as_hint(),
                er_diagram::SELECT.as_hint(),
                er_diagram::PAN.as_hint(),
                er_diagram::ZOOM.as_hint(),
                er_diagram::FIT.as_hint(),
                er_diagram::HELP.as_hint(),
                esc_hint,
            ]),
            theme,
        );
        let status = status_line(selected.as_deref(), &view.relations, diagram, theme);
        let status_height =
            wrapped_line_count(&status.to_string(), inner.width).clamp(1, MAX_STATUS_LINES);
        let [canvas_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(status_height)]).areas(inner);
        frame.render_widget(
            Paragraph::new(status).wrap(Wrap { trim: false }),
            status_area,
        );

        let Some(flow) = view.flow.as_mut() else {
            let message = view.build_error.as_deref().unwrap_or("Nothing to draw");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!("Failed to lay out ER diagram: {message}"),
                    Style::default().fg(theme.semantic.status.error),
                )),
                canvas_area,
            );
            return false;
        };
        flow.theme = Theme::Custom(flow_palette(theme));
        apply_edge_styles(
            flow,
            &view.relations,
            selected.as_deref(),
            view.show_all_fk_names,
            theme,
        );
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
        selected.is_some()
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
    let layered = flow.nodes().count() <= LAYERED_LAYOUT_MAX_TABLES;
    if layered {
        // Left-to-right keeps FK labels on horizontal segments, where they read best.
        flow.apply_layout(
            Sugiyama::horizontal()
                .with_rank_spacing(rank_spacing as f64)
                .with_node_spacing(NODE_SPACING),
        );
    }
    let main_component = pack_components(&mut flow, layered);
    let seeds: Vec<String> = seed_tables
        .iter()
        .filter(|id| flow.node(id).is_some())
        .cloned()
        .collect();
    let selected_table = match seeds.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    };
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
        selected_table,
    }
}

fn graph_flow(tables: &[ErTableInfo], relations: &[Relation]) -> Result<ErFlow, rataflow::Error> {
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
    if let Some(table) = &layout.selected_table {
        flow.select_node(table);
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
fn relation_edges(
    relations: &[Relation],
    is_known: impl Fn(&str) -> bool,
) -> Vec<Edge<FloatingEdge>> {
    let by_pair = relations_by_pair(relations);

    relations
        .iter()
        .filter(|r| r.from != r.to && is_known(&r.from) && is_known(&r.to))
        .enumerate()
        .filter_map(|(i, r)| {
            let reverse = by_pair.get(&(r.to.as_str(), r.from.as_str()));
            if reverse.is_some() && r.from > r.to {
                return None;
            }
            Some(
                Edge::new(format!("fk-{i}"), r.from.as_str(), r.to.as_str())
                    .with_label(edge_label(&by_pair, &r.from, &r.to))
                    .with_content(
                        FloatingEdge::stepped().with_attachment(FloatingAttachment::Perimeter),
                    ),
            )
        })
        .collect()
}

fn relations_by_pair(relations: &[Relation]) -> HashMap<(&str, &str), &Relation> {
    relations
        .iter()
        .map(|r| ((r.from.as_str(), r.to.as_str()), r))
        .collect()
}

fn edge_label(by_pair: &HashMap<(&str, &str), &Relation>, from: &str, to: &str) -> String {
    [(from, to), (to, from)]
        .iter()
        .filter_map(|pair| by_pair.get(pair))
        .flat_map(|r| &r.fk_names)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Sugiyama lays out each connected component from the origin, so components
/// (and FK-less tables) would pile up on top of each other. Arrange them in
/// rows instead, largest first, and return the ids of the largest component.
/// Without a layered layout every table is packed on its own.
fn pack_components(flow: &mut ErFlow, keep_connected: bool) -> Vec<String> {
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

fn table_nearest_view_center(flow: &ErFlow) -> Option<String> {
    let canvas = flow.canvas_size();
    let viewport = &flow.viewport;
    let center = Position::new(
        (canvas.width / 2.0 - viewport.x) / viewport.zoom,
        (canvas.height / 2.0 - viewport.y) / viewport.zoom,
    );
    flow.nodes()
        .map(|node| {
            let dx = node.position.x + node.width / 2.0 - center.x;
            let dy = node.position.y + node.height / 2.0 - center.y;
            (node.id.clone(), dx.hypot(dy))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
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
        muted: theme.semantic.text.secondary,
        subtle: theme.semantic.text.dim,
        accent: theme.semantic.surface.focus_border,
        text: theme.semantic.text.primary,
        success: theme.semantic.status.success,
        error: theme.semantic.status.error,
    }
}

/// Edges touching the selected table are drawn in the accent color and carry
/// their FK names, and the rest are hidden so its relationships read clearly.
/// Without a selection edges are labeled only on request: labels do not
/// shrink when zoomed out and quickly bury dense diagrams.
fn apply_edge_styles(
    flow: &mut ErFlow,
    relations: &[Relation],
    selected: Option<&str>,
    show_all_fk_names: bool,
    theme: &ThemePalette,
) {
    let by_pair = relations_by_pair(relations);
    let edges: Vec<(String, bool, bool, Option<String>, bool)> = flow
        .edges()
        .iter()
        .map(|edge| {
            let (source, target) = (edge.source.as_str(), edge.target.as_str());
            let highlighted = selected.is_some_and(|id| source == id || target == id);
            let bidirectional = by_pair.contains_key(&(target, source));
            let label =
                (highlighted || show_all_fk_names).then(|| edge_label(&by_pair, source, target));
            let hidden = selected.is_some() && !highlighted && !show_all_fk_names;
            (edge.id.clone(), highlighted, bidirectional, label, hidden)
        })
        .collect();
    for (id, highlighted, bidirectional, label_text, hidden) in edges {
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
            // rataflow points start markers away from the source; point back into it.
            style = style.with_marker_start(EdgeMarker::CustomDirectional {
                left: '▶',
                right: '◀',
                top: '▼',
                bottom: '▲',
            });
        }
        if let Some(content) = flow.edge_content_mut(&id) {
            content.style = Some(style);
        }
        flow.set_edge_label(&id, label_text);
        flow.set_edge_hidden(&id, hidden);
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
        let shown: HashSet<&str> = diagram
            .tables
            .iter()
            .map(|t| t.qualified_name.as_str())
            .collect();
        let fk_count: usize = relations
            .iter()
            .filter(|r| shown.contains(r.from.as_str()) && shown.contains(r.to.as_str()))
            .map(|r| r.fk_names.len())
            .sum();
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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
            has_selection: false,
        }
    }

    fn assert_all_tables_on_screen(flow: &ErFlow, width: i32, height: i32) {
        for node in flow.nodes() {
            let (left, top) = flow.world_to_terminal(node.position);
            let (right, bottom) = flow.world_to_terminal(Position::new(
                node.position.x + node.width,
                node.position.y + node.height,
            ));
            assert!(
                left >= 0 && top >= 0 && right <= width && bottom <= height,
                "{} is cut off",
                node.id
            );
        }
    }

    fn render_flow(flow: &mut ErFlow, width: u16, height: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(&mut *flow, frame.area()))
            .unwrap();
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
            .find(|e| e.connects_directed("public.posts", "public.users"))
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

        let flow = graph_flow(&tables, &relations).unwrap();

        let content = &flow.node("public.comments").unwrap().content;
        assert_eq!(content.name, "comments");
        assert_eq!(content.self_reference_fks, ["fk_parent"]);
    }

    fn assert_no_overlap(tables: &[ErTableInfo], layout: &ErLayout) {
        let flow = build_flow(tables, &collect_relations(tables), layout).unwrap();
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
    fn layout_places_every_table_without_overlap() {
        let tables = sample_tables();

        let layout = layout_tables(&tables, &[]);

        assert_no_overlap(&tables, &layout);
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
            has_selection: false,
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
    fn full_diagram_starts_without_selection() {
        let tables = vec![table("public.a", &[]), table("public.b", &[])];

        let layout = layout_tables(&tables, &[]);

        assert_eq!(layout.selected_table, None);
    }

    #[test]
    fn grid_fallback_places_tables_without_overlap() {
        let tables: Vec<ErTableInfo> = (0..=LAYERED_LAYOUT_MAX_TABLES)
            .map(|i| {
                let target = format!("public.t{}", i / 2);
                table(&format!("public.t{i}"), &[("fk", target.as_str())])
            })
            .collect();

        let layout = layout_tables(&tables, &[]);

        assert_no_overlap(&tables, &layout);
    }

    #[test]
    fn toggle_labels_every_edge() {
        let tables = sample_tables();
        let relations = collect_relations(&tables);
        let mut flow = build_flow(&tables, &relations, &layout_tables(&tables, &[])).unwrap();
        let theme = palette_for(ThemeId::Default);

        apply_edge_styles(&mut flow, &relations, None, false, theme);
        assert!(flow.edges().iter().all(|e| e.label.is_none()));

        apply_edge_styles(&mut flow, &relations, None, true, theme);
        assert!(flow.edges().iter().all(|e| e.label.is_some()));
    }

    #[test]
    fn selection_labels_only_its_edges() {
        let tables = sample_tables();
        let relations = collect_relations(&tables);
        let mut flow = build_flow(&tables, &relations, &layout_tables(&tables, &[])).unwrap();

        apply_edge_styles(
            &mut flow,
            &relations,
            Some("public.users"),
            false,
            palette_for(ThemeId::Default),
        );

        let labeled: Vec<_> = flow
            .edges()
            .iter()
            .filter_map(|e| e.label.as_deref())
            .collect();
        assert_eq!(labeled, ["fk_author, fk_editor"]);
    }

    #[test]
    fn first_directional_select_starts_from_view_center() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));
        // The initial fit centers comments → posts → users; posts sits in the middle.
        render_flow(view.flow.as_mut().unwrap(), 100, 30);

        view.apply(ErViewerCommand::SelectRight);

        assert_eq!(view.selected_table().as_deref(), Some("public.posts"));
    }

    #[test]
    fn fit_shows_every_table_of_a_large_diagram() {
        let tables: Vec<ErTableInfo> = (0..=LAYERED_LAYOUT_MAX_TABLES)
            .map(|i| table(&format!("public.t{i}"), &[]))
            .collect();
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, tables)));
        render_flow(view.flow.as_mut().unwrap(), 100, 30);

        view.apply(ErViewerCommand::FitView);
        let flow = view.flow.as_mut().unwrap();
        render_flow(flow, 100, 30);

        assert_all_tables_on_screen(flow, 100, 30);
    }

    #[test]
    fn fit_shows_every_table_of_a_long_fk_chain() {
        let names: Vec<String> = (0..200).map(|i| format!("public.{i:0>56}")).collect();
        let tables: Vec<ErTableInfo> = names
            .iter()
            .enumerate()
            .map(|(i, name)| match i.checked_sub(1) {
                Some(prev) => table(name, &[("fk", names[prev].as_str())]),
                None => table(name, &[]),
            })
            .collect();
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, tables)));
        render_flow(view.flow.as_mut().unwrap(), 100, 30);

        view.apply(ErViewerCommand::FitView);
        let flow = view.flow.as_mut().unwrap();
        render_flow(flow, 100, 30);

        assert_all_tables_on_screen(flow, 100, 30);
    }

    #[test]
    fn zoom_out_after_fit_never_zooms_in() {
        let tables: Vec<ErTableInfo> = (0..=LAYERED_LAYOUT_MAX_TABLES)
            .map(|i| table(&format!("public.t{i}"), &[]))
            .collect();
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, tables)));
        render_flow(view.flow.as_mut().unwrap(), 100, 30);
        view.apply(ErViewerCommand::FitView);
        render_flow(view.flow.as_mut().unwrap(), 100, 30);
        let fitted = view.flow.as_ref().unwrap().viewport.zoom;

        view.apply(ErViewerCommand::ZoomOut);

        assert!(view.flow.as_ref().unwrap().viewport.zoom <= fitted);
    }

    #[test]
    fn schema_is_shown_only_when_schemas_are_mixed() {
        let mut tables = sample_tables();
        let relations = collect_relations(&tables);
        let single = graph_flow(&tables, &relations).unwrap();
        tables.push(table("audit.logs", &[]));

        let mixed = graph_flow(&tables, &relations).unwrap();

        assert_eq!(single.node("public.users").unwrap().content.schema, None);
        assert_eq!(
            mixed
                .node("public.users")
                .unwrap()
                .content
                .schema
                .as_deref(),
            Some("public")
        );
    }

    #[test]
    fn zoomed_out_table_keeps_its_name_on_the_border() {
        let node = table_node(&table("public.users", &[]), &[], true);
        let area = Rect::new(0, 0, 12, 2);
        let mut buf = Buffer::empty(area);

        node.content.render(
            &NodeRenderContext {
                id: "public.users",
                area,
                selected: false,
                dragging: false,
                position_absolute: Position::new(0.0, 0.0),
                theme: Theme::default(),
                animation_phase: 0,
            },
            &mut buf,
        );

        let top: String = (0..area.width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect();
        assert!(top.contains("users"), "{top}");
    }

    #[test]
    fn selection_hides_unrelated_edges() {
        let tables = sample_tables();
        let relations = collect_relations(&tables);
        let mut flow = build_flow(&tables, &relations, &layout_tables(&tables, &[])).unwrap();

        apply_edge_styles(
            &mut flow,
            &relations,
            Some("public.users"),
            false,
            palette_for(ThemeId::Default),
        );

        let visible: Vec<_> = flow
            .edges()
            .iter()
            .filter(|e| !e.hidden)
            .map(|e| (e.source.as_str(), e.target.as_str()))
            .collect();
        assert_eq!(visible, [("public.posts", "public.users")]);
    }

    #[test]
    fn half_page_pans_by_half_the_canvas_height() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));
        render_flow(view.flow.as_mut().unwrap(), 100, 30);
        let before = view.flow.as_ref().unwrap().viewport.y;

        view.apply(ErViewerCommand::PanHalfPageDown);

        let moved = before - view.flow.as_ref().unwrap().viewport.y;
        assert!((moved - 15.0).abs() < 1.0, "moved {moved}");
    }

    #[test]
    fn clear_selection_deselects_the_table() {
        let mut view = ErDiagramView::default();
        view.sync(Some(&diagram(1, sample_tables())));
        view.apply(ErViewerCommand::SelectNext);

        view.apply(ErViewerCommand::ClearSelection);

        assert!(view.selected_table().is_none());
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
