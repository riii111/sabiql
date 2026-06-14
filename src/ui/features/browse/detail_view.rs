use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::model::shared::detail_view::DetailSearchState;
use crate::primitives::atoms::{CursorKind, set_terminal_cursor, text_cursor_spans_with_kind};
use crate::theme::ThemePalette;

pub fn search_match_status(search: &DetailSearchState) -> String {
    if search.matches().is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", search.current_match() + 1, search.matches().len())
    }
}

pub fn render_detail_search(
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
        visible_input.chars().count(),
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
