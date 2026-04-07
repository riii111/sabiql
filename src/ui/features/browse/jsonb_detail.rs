use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailMode;
use crate::app::model::shared::text_input::TextInputLike;
use crate::ui::primitives::atoms::{
    CursorKind, ModalTextSurface, build_modal_text_surface_lines, render_modal_text_surface,
    set_terminal_cursor, text_cursor_spans_with_kind,
};
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::ThemePalette;

pub struct JsonbDetailRenderMetrics {
    pub editor_visible_rows: usize,
}
pub struct JsonbDetail;

impl JsonbDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<JsonbDetailRenderMetrics> {
        if !state.jsonb_detail.is_active() {
            return None;
        }

        let is_editing = matches!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        let title = if is_editing {
            format!(
                " JSONB Edit \u{2500}\u{2500} {} (jsonb) ",
                state.jsonb_detail.column_name()
            )
        } else {
            format!(
                " JSONB Detail \u{2500}\u{2500} {}",
                state.jsonb_detail.column_name()
            )
        };
        let hint = if is_editing {
            " Esc:Normal "
        } else {
            " y:Copy  /:Search  Enter/i:Insert  Esc:Close "
        };

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            hint,
            theme,
        );

        let (editor_area, status_area, search_area) = if state.jsonb_detail.search().active {
            let [editor_area, status_area, search_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            (editor_area, status_area, Some(search_area))
        } else {
            let [editor_area, status_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            (editor_area, status_area, None)
        };

        Self::render_editor_content(frame, editor_area, state, is_editing, now, theme);
        Self::render_status(frame, status_area, state, theme);
        if let Some(search_area) = search_area {
            Self::render_search(frame, search_area, state, theme);
        }

        Some(JsonbDetailRenderMetrics {
            editor_visible_rows: editor_area.height as usize,
        })
    }

    fn render_editor_content(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        is_editing: bool,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) {
        let editor = state.jsonb_detail.editor();
        let content = editor.content();
        let (cursor_row, cursor_col) = editor.cursor_to_position();
        let cursor_kind = if is_editing {
            CursorKind::Insert
        } else {
            CursorKind::Block
        };
        let surface = ModalTextSurface {
            content,
            cursor_row,
            cursor_col,
            scroll_row: editor.scroll_row(),
            cursor_kind,
            empty_placeholder: if is_editing {
                " Enter JSON..."
            } else {
                " Press Enter or i to edit..."
            },
            base_style: Style::default().fg(theme.text_primary),
            current_line_style: Style::default().bg(theme.editor_current_line_bg),
        };

        let line_spans: Vec<Vec<Span<'static>>> = content
            .lines()
            .map(|line| vec![Span::raw(line.to_owned())])
            .collect();
        let mut lines = build_modal_text_surface_lines(surface, line_spans, theme);

        let flash_active = state.flash_timers.is_active(
            crate::app::model::shared::flash_timer::FlashId::JsonbDetail,
            now,
        );
        crate::ui::primitives::atoms::apply_yank_flash(&mut lines, flash_active, theme);

        render_modal_text_surface(frame, area, surface, lines);
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let mut spans = Vec::new();

        if state.jsonb_detail.has_pending_changes() {
            spans.push(Span::styled(
                "\u{25cf} Modified  ",
                Style::default().fg(theme.cell_draft_pending_fg),
            ));
        }

        if let Some(err) = state.jsonb_detail.validation_error() {
            spans.push(Span::styled(
                format!("\u{2717} {err}"),
                Style::default().fg(theme.status_error),
            ));
        } else {
            spans.push(Span::styled(
                "\u{2713} Valid JSON",
                Style::default().fg(theme.status_success),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_search(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let search = state.jsonb_detail.search();
        let input = search.input.content();
        let cursor = search.input.cursor();
        let match_info = if search.matches.is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", search.current_match + 1, search.matches.len())
        };
        let suffix = format!("  {match_info}");
        let visible_width = area
            .width
            .saturating_sub((1 + UnicodeWidthStr::width(suffix.as_str())) as u16)
            as usize;
        let viewport_offset = search_viewport_offset(input, cursor, visible_width);
        let visible_input = slice_chars_fitting_width(input, viewport_offset, visible_width);
        let relative_cursor = cursor.saturating_sub(viewport_offset);

        let mut spans = vec![Span::styled("/", Style::default().fg(theme.text_accent))];
        spans.extend(text_cursor_spans_with_kind(
            &visible_input,
            relative_cursor,
            0,
            visible_input.chars().count(),
            CursorKind::Insert,
            theme,
        ));
        spans.push(Span::styled(suffix, Style::default().fg(theme.text_muted)));

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        set_terminal_cursor(frame, area, &visible_input, 0, relative_cursor, 0, 1);
    }
}

fn search_viewport_offset(input: &str, cursor: usize, visible_width: usize) -> usize {
    if visible_width == 0 {
        return cursor;
    }

    let chars: Vec<char> = input.chars().collect();
    let mut viewport_offset = 0;
    let mut width_before_cursor = display_width(&chars[..cursor.min(chars.len())]);

    while width_before_cursor >= visible_width && viewport_offset < cursor {
        width_before_cursor =
            width_before_cursor.saturating_sub(char_width(chars[viewport_offset]));
        viewport_offset += 1;
    }

    viewport_offset
}

fn slice_chars_fitting_width(input: &str, start: usize, visible_width: usize) -> String {
    if visible_width == 0 {
        return String::new();
    }

    let mut width = 0;
    let mut visible = String::new();

    for ch in input.chars().skip(start) {
        let ch_width = char_width(ch);
        if width + ch_width > visible_width {
            break;
        }
        width += ch_width;
        visible.push(ch);
    }

    visible
}

fn display_width(chars: &[char]) -> usize {
    chars.iter().map(|&ch| char_width(ch)).sum()
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::slice_chars_fitting_width;

    #[test]
    fn slice_chars_fitting_width_omits_first_wide_char_when_viewport_is_too_narrow() {
        assert_eq!(slice_chars_fitting_width("界a", 0, 1), "");
    }

    #[test]
    fn slice_chars_fitting_width_keeps_chars_that_fit_exactly() {
        assert_eq!(slice_chars_fitting_width("ab", 0, 2), "ab");
    }
}
