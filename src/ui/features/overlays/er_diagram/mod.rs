mod content;
mod graph;

use std::collections::HashSet;

use rataflow::{
    ControlsAction, EdgeMarker, EdgeStyle, EventResponse, FitViewOptions, FlowAction, Palette,
    Position, Theme,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::er_state::{ErDiagram, ErViewerCommand};
use crate::app::update::input::keybindings::er_diagram;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::primitives::utils::text_utils::wrapped_line_count;
use crate::theme::ThemePalette;
use graph::{ErFlow, Relation, collect_relations, table_graph};

pub use graph::layout_tables;

const MIN_ZOOM: f64 = 0.2;
const FIT_PADDING: f64 = 1.0;
const MAX_STATUS_LINES: u16 = 3;

/// Renderer-owned state of the ER diagram viewer.
///
/// The app layer only holds the diagram data; layout, viewport and selection
/// live in the rataflow `Flow`, which is rebuilt whenever a new diagram arrives.
#[derive(Default)]
pub struct ErDiagramView {
    diagram_id: Option<u64>,
    /// `None` until a diagram arrives; `Err` when it could not be drawn.
    flow: Option<Result<ErFlow, String>>,
    relations: Vec<Relation>,
    drawn_fk_count: usize,
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
        let shown: HashSet<&str> = diagram
            .tables
            .iter()
            .map(|table| table.qualified_name.as_str())
            .collect();
        let drawn_fk_count = relations
            .iter()
            .filter(|r| shown.contains(r.from.as_str()) && shown.contains(r.to.as_str()))
            .map(|r| r.fk_names.len())
            .sum();
        let flow = positioned_graph(diagram, &relations).map_err(|e| e.to_string());
        *self = Self {
            diagram_id: Some(diagram.id),
            flow: Some(flow),
            relations,
            drawn_fk_count,
            show_all_fk_names: false,
        };
    }

    pub fn apply(&mut self, command: ErViewerCommand) {
        let Some(Ok(flow)) = self.flow.as_mut() else {
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
            ErViewerCommand::PanHalfPageDown => pan_half_page(flow, 1.0),
            ErViewerCommand::PanHalfPageUp => pan_half_page(flow, -1.0),
            ErViewerCommand::ClearSelection => flow.apply(FlowAction::ClearSelection),
            ErViewerCommand::CenterOnSelected => flow.apply(FlowAction::CenterOnSelected),
            ErViewerCommand::ZoomIn => flow.apply_controls_action(ControlsAction::ZoomIn),
            ErViewerCommand::ZoomOut => flow.apply_controls_action(ControlsAction::ZoomOut),
            ErViewerCommand::ResetZoom => flow.apply_controls_action(ControlsAction::ResetZoom),
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
            ErViewerCommand::ToggleFkNames => {
                self.show_all_fk_names = !self.show_all_fk_names;
                EventResponse::Handled
            }
        };
    }

    fn selected_table(&self) -> Option<String> {
        self.flow.as_ref()?.as_ref().ok()?.first_selected_node_id()
    }
}

/// `direction` is 1.0 for down and -1.0 for up.
fn pan_half_page(flow: &mut ErFlow, direction: f64) -> EventResponse {
    let dy = flow.canvas_size().height / 2.0 * direction;
    flow.apply(FlowAction::Pan { dx: 0.0, dy })
}

fn table_nearest_view_center(flow: &ErFlow) -> Option<String> {
    let canvas = flow.canvas_size();
    let center = flow
        .viewport
        .canvas_to_world(Position::new(canvas.width / 2.0, canvas.height / 2.0));
    flow.nodes()
        .map(|node| (node.id.clone(), node.bounds().center().distance_to(&center)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The diagram's graph at its precomputed positions, ready to show.
fn positioned_graph(
    diagram: &ErDiagram,
    relations: &[Relation],
) -> Result<ErFlow, rataflow::Error> {
    let mut flow = table_graph(&diagram.tables, relations)?.with_min_zoom(MIN_ZOOM);
    flow.set_node_positions(
        diagram
            .layout
            .positions
            .iter()
            .map(|(id, &(x, y))| (id, Position::new(x, y))),
    );

    let ids: Vec<String> = flow.nodes().map(|node| node.id.clone()).collect();
    for id in &ids {
        flow.set_handles_hidden(id, true);
    }
    if let Some(table) = &diagram.selected_table {
        flow.select_node(table);
    }
    // Start at full size so names stay legible; `f` zooms out to show everything.
    flow.request_fit_view_with_options(FitViewOptions {
        padding: FIT_PADDING,
        min_zoom: Some(1.0),
        max_zoom: Some(1.0),
        // The picked table of a partial diagram, otherwise the whole diagram.
        nodes: diagram.selected_table.iter().cloned().collect(),
        ..FitViewOptions::default()
    });
    Ok(flow)
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
            Constraint::Percentage(100),
            Constraint::Percentage(100),
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
        let status = status_line(
            selected.as_deref(),
            &view.relations,
            diagram.tables.len(),
            view.drawn_fk_count,
            theme,
        );
        let status_height =
            wrapped_line_count(&status.to_string(), inner.width).clamp(1, MAX_STATUS_LINES);
        let [canvas_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(status_height)]).areas(inner);
        frame.render_widget(
            Paragraph::new(status).wrap(Wrap { trim: false }),
            status_area,
        );

        let flow = match view.flow.as_mut() {
            Some(Ok(flow)) => flow,
            Some(Err(message)) => {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!("Failed to lay out ER diagram: {message}"),
                        Style::default().fg(theme.semantic.status.error),
                    )),
                    canvas_area,
                );
                return false;
            }
            None => return false,
        };
        flow.theme = Theme::Custom(flow_palette(theme));
        apply_edge_styles(flow, selected.as_deref(), view.show_all_fk_names, theme);
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

/// Edges touching the selected table are drawn in the accent color with their
/// FK names, and the rest are hidden so its relationships read clearly. `n`
/// (`show_all_fk_names`) shows every edge with its FK names instead; otherwise
/// edges stay unlabeled, since labels do not shrink when zoomed out.
fn apply_edge_styles(
    flow: &mut ErFlow,
    selected: Option<&str>,
    show_all_fk_names: bool,
    theme: &ThemePalette,
) {
    let accent = Style::default().fg(theme.semantic.surface.focus_border);
    // Labels share the stroke color: rataflow recolors label cells that a later
    // edge crosses, which would otherwise leave mismatched letters.
    let muted = Style::default().fg(theme.component.modal.border);

    let ids: Vec<String> = flow.edges().iter().map(|edge| edge.id.clone()).collect();
    for id in ids {
        let Some(edge) = flow.edge(&id) else {
            continue;
        };
        let highlighted =
            selected.is_some_and(|table| edge.source == table || edge.target == table);
        let hidden = selected.is_some() && !highlighted && !show_all_fk_names;
        let label = (highlighted || show_all_fk_names).then(|| edge.content.label.clone());
        let (stroke, label_style) = if highlighted {
            (accent, accent.add_modifier(Modifier::BOLD))
        } else {
            (muted, muted)
        };
        let mut style = EdgeStyle::default()
            .with_stroke_style(stroke)
            .with_label_style(label_style);
        if edge.content.bidirectional {
            // rataflow points start markers away from the source; point back into it.
            style = style.with_marker_start(EdgeMarker::CustomDirectional {
                left: '▶',
                right: '◀',
                top: '▼',
                bottom: '▲',
            });
        }
        if let Some(content) = flow.edge_content_mut(&id) {
            content.style = style;
        }
        flow.set_edge_label(&id, label);
        flow.set_edge_hidden(&id, hidden);
    }
}

fn status_line(
    selected: Option<&str>,
    relations: &[Relation],
    table_count: usize,
    drawn_fk_count: usize,
    theme: &ThemePalette,
) -> Line<'static> {
    let muted = Style::default().fg(theme.semantic.text.muted);
    let Some(selected) = selected else {
        return Line::from(Span::styled(
            format!(
                " {table_count} tables · {drawn_fk_count} foreign keys — press {} to select a table and show its foreign keys",
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
mod test_support {
    use crate::domain::ErTableInfo;
    use crate::domain::er::ErFkInfo;

    pub(super) fn table(qualified: &str, fks: &[(&str, &str)]) -> ErTableInfo {
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

    pub(super) fn sample_tables() -> Vec<ErTableInfo> {
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

    pub(super) fn isolated_tables(count: usize) -> Vec<ErTableInfo> {
        (0..count)
            .map(|i| table(&format!("public.t{i}"), &[]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{isolated_tables, sample_tables, table};
    use super::*;
    use crate::app::model::shared::theme_id::ThemeId;
    use crate::domain::ErTableInfo;
    use crate::theme::palette_for;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn diagram(id: u64, tables: Vec<ErTableInfo>) -> ErDiagram {
        let total_tables = tables.len();
        let layout = layout_tables(&tables);
        ErDiagram {
            id,
            tables,
            total_tables,
            layout,
            selected_table: None,
            has_selection: false,
        }
    }

    fn synced_view(diagram: &ErDiagram) -> ErDiagramView {
        let mut view = ErDiagramView::default();
        view.sync(Some(diagram));
        view
    }

    fn flow(view: &mut ErDiagramView) -> &mut ErFlow {
        view.flow.as_mut().unwrap().as_mut().unwrap()
    }

    fn render(view: &mut ErDiagramView, width: u16, height: u16) {
        let flow = flow(view);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(&mut *flow, frame.area()))
            .unwrap();
    }

    /// The terminal cells covered by every table, as (left, top, right, bottom).
    fn tables_on_screen(flow: &ErFlow) -> (i32, i32, i32, i32) {
        flow.nodes().fold(
            (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
            |(left, top, right, bottom), node| {
                let bounds = node.bounds();
                let (x0, y0) = flow.world_to_terminal(bounds.position);
                let (x1, y1) =
                    flow.world_to_terminal(Position::new(bounds.right(), bounds.bottom()));
                (left.min(x0), top.min(y0), right.max(x1), bottom.max(y1))
            },
        )
    }

    fn styled_edges(selected: Option<&str>, show_all_fk_names: bool) -> ErFlow {
        let mut view = synced_view(&diagram(1, sample_tables()));
        let mut flow = view.flow.take().unwrap().unwrap();
        apply_edge_styles(
            &mut flow,
            selected,
            show_all_fk_names,
            palette_for(ThemeId::Default),
        );
        flow
    }

    #[test]
    fn same_diagram_keeps_selection() {
        let diagram = diagram(1, sample_tables());
        let mut view = synced_view(&diagram);
        assert!(view.selected_table().is_none());
        view.apply(ErViewerCommand::SelectNext);

        view.sync(Some(&diagram));

        assert!(view.selected_table().is_some());
    }

    #[test]
    fn new_diagram_resets_selection() {
        let mut view = synced_view(&diagram(1, sample_tables()));
        view.apply(ErViewerCommand::SelectNext);

        view.sync(Some(&diagram(2, sample_tables())));

        assert!(view.selected_table().is_none());
    }

    #[test]
    fn picked_table_starts_selected() {
        let diagram = ErDiagram {
            selected_table: Some("public.posts".to_string()),
            ..diagram(1, sample_tables())
        };

        let view = synced_view(&diagram);

        assert_eq!(view.selected_table().as_deref(), Some("public.posts"));
    }

    #[test]
    fn clear_selection_deselects_the_table() {
        let mut view = synced_view(&diagram(1, sample_tables()));
        view.apply(ErViewerCommand::SelectNext);

        view.apply(ErViewerCommand::ClearSelection);

        assert!(view.selected_table().is_none());
    }

    #[test]
    fn closed_diagram_drops_flow() {
        let mut view = synced_view(&diagram(1, sample_tables()));

        view.sync(None);

        assert!(view.flow.is_none());
        assert!(view.diagram_id.is_none());
    }

    #[test]
    fn initial_view_centers_the_whole_diagram() {
        let mut view = synced_view(&diagram(1, sample_tables()));

        render(&mut view, 200, 60);

        let (left, top, right, bottom) = tables_on_screen(flow(&mut view));
        assert!(
            (i32::midpoint(left, right) - 100).abs() <= 1,
            "x: {left}..{right}"
        );
        assert!(
            (i32::midpoint(top, bottom) - 30).abs() <= 1,
            "y: {top}..{bottom}"
        );
    }

    #[test]
    fn first_directional_select_starts_from_view_center() {
        // comments → posts → users is the widest row, so posts sits at the center.
        let mut view = synced_view(&diagram(1, sample_tables()));
        render(&mut view, 100, 30);

        view.apply(ErViewerCommand::SelectRight);

        assert_eq!(view.selected_table().as_deref(), Some("public.posts"));
    }

    #[test]
    fn half_page_pans_by_half_the_canvas_height() {
        let mut view = synced_view(&diagram(1, sample_tables()));
        render(&mut view, 100, 30);
        let before = flow(&mut view).viewport.y;

        view.apply(ErViewerCommand::PanHalfPageDown);

        let moved = before - flow(&mut view).viewport.y;
        assert!((moved - 15.0).abs() < 1.0, "moved {moved}");
    }

    fn assert_fit_shows_every_table(tables: Vec<ErTableInfo>) {
        let mut view = synced_view(&diagram(1, tables));
        render(&mut view, 100, 30);

        view.apply(ErViewerCommand::FitView);
        render(&mut view, 100, 30);

        let (left, top, right, bottom) = tables_on_screen(flow(&mut view));
        assert!(
            left >= 0 && top >= 0 && right <= 100 && bottom <= 30,
            "{left},{top}..{right},{bottom}"
        );
    }

    #[test]
    fn fit_shows_every_table_of_a_large_diagram() {
        assert_fit_shows_every_table(isolated_tables(graph::LAYERED_LAYOUT_MAX_TABLES + 1));
    }

    #[test]
    fn fit_shows_every_table_of_a_long_fk_chain() {
        let names: Vec<String> = (0..200).map(|i| format!("public.{i:0>56}")).collect();
        let tables = names
            .iter()
            .enumerate()
            .map(|(i, name)| match i.checked_sub(1) {
                Some(prev) => table(name, &[("fk", names[prev].as_str())]),
                None => table(name, &[]),
            })
            .collect();

        assert_fit_shows_every_table(tables);
    }

    #[test]
    fn zoom_out_after_fit_never_zooms_in() {
        let mut view = synced_view(&diagram(
            1,
            isolated_tables(graph::LAYERED_LAYOUT_MAX_TABLES + 1),
        ));
        render(&mut view, 100, 30);
        view.apply(ErViewerCommand::FitView);
        render(&mut view, 100, 30);
        let fitted = flow(&mut view).viewport.zoom;

        view.apply(ErViewerCommand::ZoomOut);

        assert!(flow(&mut view).viewport.zoom <= fitted);
    }

    #[test]
    fn edges_stay_unlabeled_without_selection() {
        let flow = styled_edges(None, false);

        assert!(flow.edges().iter().all(|e| e.label.is_none() && !e.hidden));
    }

    #[test]
    fn fk_names_toggle_labels_every_edge() {
        let mut view = synced_view(&diagram(1, sample_tables()));

        view.apply(ErViewerCommand::ToggleFkNames);

        assert!(view.show_all_fk_names);
        let mut flow = view.flow.take().unwrap().unwrap();
        apply_edge_styles(&mut flow, None, true, palette_for(ThemeId::Default));
        assert!(flow.edges().iter().all(|e| e.label.is_some()));
    }

    #[test]
    fn selection_shows_and_labels_only_its_edges() {
        let flow = styled_edges(Some("public.users"), false);

        let visible: Vec<_> = flow
            .edges()
            .iter()
            .filter(|e| !e.hidden)
            .map(|e| (e.source.as_str(), e.target.as_str(), e.label.as_deref()))
            .collect();
        assert_eq!(
            visible,
            [("public.posts", "public.users", Some("fk_author, fk_editor"))]
        );
    }

    #[test]
    fn status_line_lists_outgoing_and_incoming_fks() {
        let relations = collect_relations(&sample_tables());

        let line = status_line(
            Some("public.posts"),
            &relations,
            6,
            6,
            palette_for(ThemeId::Default),
        );

        assert_eq!(
            line.to_string(),
            " public.posts  → public.users (fk_author, fk_editor)  ← public.comments (fk_post)"
        );
    }
}
