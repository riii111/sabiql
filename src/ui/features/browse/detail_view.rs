use crate::app::model::shared::text_input::input_viewport_offset;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::model::shared::detail_view::DetailSearchState;
use crate::primitives::atoms::{CursorKind, set_terminal_cursor, text_cursor_spans_with_kind};
use crate::theme::ThemePalette;

pub(super) fn search_match_status(search: &DetailSearchState) -> String {
    if search.matches().is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", search.current_match() + 1, search.matches().len())
    }
}

pub(super) fn render_detail_search(
    frame: &mut Frame,
    area: Rect,
    search: &DetailSearchState,
    theme: &ThemePalette,
) {
    let input = search.input().content();
    let cursor = search.input().cursor();
    let suffix = format!("  {}", search_match_status(search));
    let visible_width =
        area.width
            .saturating_sub((1 + UnicodeWidthStr::width(suffix.as_str())) as u16) as usize;
    let viewport_offset = search_viewport_offset(input, cursor, visible_width);
    let visible_input = slice_chars_fitting_width(input, viewport_offset, visible_width);
    let relative_cursor = cursor.saturating_sub(viewport_offset);

    let mut spans = vec![Span::styled(
        "/",
        Style::default().fg(theme.semantic.text.accent),
    )];
    spans.extend(text_cursor_spans_with_kind(
        &visible_input,
        relative_cursor,
        0,
        visible_width,
        CursorKind::Insert,
        theme,
    ));
    spans.push(Span::styled(
        suffix,
        Style::default().fg(theme.semantic.text.muted),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    set_terminal_cursor(frame, area, &visible_input, 0, relative_cursor, 0, 1);
}

fn search_viewport_offset(input: &str, cursor: usize, visible_width: usize) -> usize {
    input_viewport_offset(input, cursor, 0, visible_width, false)
}

fn slice_chars_fitting_width(input: &str, start: usize, visible_width: usize) -> String {
    if visible_width == 0 {
        return String::new();
    }

    let mut width = 0usize;
    let mut char_index = 0usize;
    let mut visible = String::new();
    for grapheme in input.graphemes(true) {
        char_index += grapheme.chars().count();
        if char_index <= start {
            continue;
        }
        let next = width.saturating_add(UnicodeWidthStr::width(grapheme));
        if next > visible_width {
            break;
        }
        width = next;
        visible.push_str(grapheme);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::{search_viewport_offset, slice_chars_fitting_width};

    #[test]
    fn search_viewport_offset_keeps_exact_fit_at_start() {
        assert_eq!(search_viewport_offset("ab", 2, 2), 0);
    }

    #[test]
    fn slice_chars_fitting_width_omits_first_wide_char_when_viewport_is_too_narrow() {
        assert_eq!(slice_chars_fitting_width("界a", 0, 1), "");
    }

    #[test]
    fn search_keeps_joined_emoji_intact() {
        assert_eq!(slice_chars_fitting_width("👩‍💻x", 0, 2), "👩‍💻");
        assert_eq!(search_viewport_offset("👩‍💻x", 3, 2), 0);
    }

    #[test]
    fn slice_chars_fitting_width_keeps_chars_that_fit_exactly() {
        assert_eq!(slice_chars_fitting_width("ab", 0, 2), "ab");
    }
}
