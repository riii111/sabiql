use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailMode;
use crate::app::policy::json::visible_line_indices;
use crate::ui::primitives::atoms::json_tree::json_tree_line_spans;
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::Theme;

pub struct JsonbDetail;

impl JsonbDetail {
    pub fn render(frame: &mut Frame, state: &AppState) {
        if !state.jsonb_detail.is_active() {
            return;
        }

        match state.jsonb_detail.mode() {
            JsonbDetailMode::Viewing | JsonbDetailMode::Searching => {
                Self::render_viewing(frame, state);
            }
            JsonbDetailMode::Editing => Self::render_editing(frame, state),
        }
    }

    fn render_viewing(frame: &mut Frame, state: &AppState) {
        let title = format!(
            " JSONB Detail \u{2500}\u{2500} {} (jsonb) ",
            state.jsonb_detail.column_name()
        );
        let is_searching = state.jsonb_detail.search().active;
        let hint = if is_searching {
            " Enter:Confirm  Esc:Cancel "
        } else {
            " y:Copy  i:Edit  /:Search  j/k:Nav  h/l:Fold  Esc:Close "
        };

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            hint,
        );

        let (tree_area, search_area) = if is_searching {
            let [t, s] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            (t, Some(s))
        } else {
            (inner, None)
        };

        let tree = state.jsonb_detail.tree();
        let visible = visible_line_indices(tree);
        let search = state.jsonb_detail.search();
        let selected = state.jsonb_detail.selected_line();
        let scroll = state.jsonb_detail.scroll_offset();
        let viewport_height = tree_area.height as usize;

        let lines: Vec<Line<'_>> = visible
            .iter()
            .skip(scroll)
            .take(viewport_height)
            .enumerate()
            .map(|(view_idx, &real_idx)| {
                let is_selected = (scroll + view_idx) == selected;
                json_tree_line_spans(&tree.lines()[real_idx], is_selected)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, tree_area);

        if let Some(area) = search_area {
            let query = search.input.content();
            let match_info = if search.matches.is_empty() {
                if query.is_empty() {
                    String::new()
                } else {
                    " [no matches]".to_string()
                }
            } else {
                format!(" [{}/{}]", search.current_match + 1, search.matches.len())
            };

            let line = Line::from(vec![
                Span::styled("/", Style::default().fg(Theme::TEXT_ACCENT)),
                Span::raw(query.to_string()),
                Span::styled(match_info, Style::default().fg(Theme::TEXT_DIM)),
            ]);
            frame.render_widget(Paragraph::new(line), area);
        }
    }

    fn render_editing(frame: &mut Frame, state: &AppState) {
        let title = format!(
            " JSONB Edit \u{2500}\u{2500} {} (jsonb) ",
            state.jsonb_detail.column_name()
        );
        let hint = " Ctrl+Enter:Save  Esc:Back ";

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            hint,
        );

        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        Self::render_editor_content(frame, editor_area, state);
        Self::render_validation_status(frame, status_area, state);
    }

    fn render_editor_content(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = state.jsonb_detail.editor().content();
        let scroll_row = state.jsonb_detail.editor().scroll_row();
        let viewport_height = area.height as usize;

        let lines: Vec<Line<'_>> = content
            .lines()
            .skip(scroll_row)
            .take(viewport_height)
            .map(|line_str| Line::from(Span::raw(line_str.to_string())))
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_validation_status(frame: &mut Frame, area: Rect, state: &AppState) {
        let line = if let Some(err) = state.jsonb_detail.validation_error() {
            Line::from(Span::styled(
                format!("\u{2717} {err}"),
                Style::default().fg(Theme::STATUS_ERROR),
            ))
        } else {
            Line::from(Span::styled(
                "\u{2713} Valid JSON".to_string(),
                Style::default().fg(Theme::STATUS_SUCCESS),
            ))
        };

        frame.render_widget(Paragraph::new(line), area);
    }
}
