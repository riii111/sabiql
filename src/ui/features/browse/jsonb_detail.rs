use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::json_tree::json_tree_line_spans;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailMode;
use crate::app::model::shared::text_input::TextInputLike;
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::Theme;

pub struct JsonbDetail;

impl JsonbDetail {
    pub fn render(frame: &mut Frame, state: &AppState, now: std::time::Instant) -> Option<usize> {
        if !state.jsonb_detail.is_active() {
            return None;
        }

        match state.jsonb_detail.mode() {
            JsonbDetailMode::Viewing | JsonbDetailMode::Searching => {
                Some(Self::render_viewing(frame, state, now))
            }
            JsonbDetailMode::Editing => {
                Self::render_editing(frame, state);
                None
            }
        }
    }

    fn render_viewing(frame: &mut Frame, state: &AppState, now: std::time::Instant) -> usize {
        let title = format!(
            " JSONB Detail \u{2500}\u{2500} {}",
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

        let has_changes = state.jsonb_detail.has_pending_changes();
        let bottom_rows = usize::from(has_changes) + usize::from(is_searching);
        let (tree_area, bottom_area) = if bottom_rows > 0 && inner.height >= 2 {
            let [t, b] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(bottom_rows as u16)])
                    .areas(inner);
            (t, Some(b))
        } else {
            (inner, None)
        };

        let viewport_height = tree_area.height as usize;
        let scroll = state.jsonb_detail.adjusted_scroll(viewport_height);

        let tree = state.jsonb_detail.tree();
        let visible = state.jsonb_detail.visible_indices();
        let search = state.jsonb_detail.search();
        let selected = state.jsonb_detail.selected_line();
        let mut lines: Vec<Line<'_>> = visible
            .iter()
            .skip(scroll)
            .take(viewport_height)
            .enumerate()
            .map(|(view_idx, &real_idx)| {
                let is_selected = (scroll + view_idx) == selected;
                json_tree_line_spans(&tree.lines()[real_idx], is_selected)
            })
            .collect();

        let flash_active = state.flash_timers.is_active(
            crate::app::model::shared::flash_timer::FlashId::JsonbDetail,
            now,
        );
        crate::ui::primitives::atoms::apply_yank_flash(&mut lines, flash_active);

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Theme::TEXT_PRIMARY));
        frame.render_widget(paragraph, tree_area);

        if let Some(bottom) = bottom_area {
            let mut bottom_lines: Vec<Line<'_>> = Vec::new();

            if has_changes {
                bottom_lines.push(Line::from(Span::styled(
                    "\u{25cf} Modified",
                    Style::default().fg(Theme::CELL_DRAFT_PENDING_FG),
                )));
            }

            if is_searching {
                let query = search.input.content();
                let cursor_pos = search.input.cursor();
                let match_info: Span<'_> = if search.matches.is_empty() {
                    if query.is_empty() {
                        Span::raw("")
                    } else {
                        Span::styled(" [no matches]", Style::default().fg(Theme::TEXT_DIM))
                    }
                } else {
                    Span::styled(
                        format!(" [{}/{}]", search.current_match + 1, search.matches.len()),
                        Style::default().fg(Theme::TEXT_DIM),
                    )
                };
                let prefix = Span::styled("/", Style::default().fg(Theme::TEXT_ACCENT));
                let cursor_spans = crate::ui::primitives::atoms::text_cursor_spans(
                    query,
                    cursor_pos,
                    search.input.viewport_offset(),
                    bottom.width.saturating_sub(1) as usize,
                );
                let mut spans = vec![prefix];
                spans.extend(cursor_spans);
                spans.push(match_info);
                bottom_lines.push(Line::from(spans));
            }

            frame.render_widget(Paragraph::new(bottom_lines), bottom);
        }

        scroll
    }

    fn render_editing(frame: &mut Frame, state: &AppState) {
        let title = format!(
            " JSONB Edit \u{2500}\u{2500} {} (jsonb) ",
            state.jsonb_detail.column_name()
        );
        let hint = " Esc:Back (applies valid changes) ";

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
        let editor = state.jsonb_detail.editor();
        let content = editor.content();
        let scroll_row = editor.scroll_row();
        let viewport_height = area.height as usize;
        let (cursor_row, cursor_col) = editor.cursor_to_position();
        let current_line_style = Style::default().bg(Theme::EDITOR_CURRENT_LINE_BG);

        let mut lines: Vec<Line<'_>> = content
            .lines()
            .enumerate()
            .skip(scroll_row)
            .take(viewport_height)
            .map(|(row, line_str)| {
                if row == cursor_row {
                    Line::from(crate::ui::primitives::atoms::text_cursor_spans(
                        line_str,
                        cursor_col,
                        0,
                        usize::MAX,
                    ))
                    .style(current_line_style)
                } else {
                    Line::from(Span::raw(line_str.to_owned()))
                }
            })
            .collect();

        // Cursor on empty trailing line after final newline
        if content.ends_with('\n') && cursor_row == content.lines().count() {
            lines.push(
                Line::from(vec![Span::styled(
                    "\u{258F}",
                    Style::default().fg(Theme::CURSOR_FG),
                )])
                .style(current_line_style),
            );
        }

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Theme::TEXT_PRIMARY));
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
                "\u{2713} Valid JSON",
                Style::default().fg(Theme::STATUS_SUCCESS),
            ))
        };

        frame.render_widget(Paragraph::new(line), area);
    }
}
