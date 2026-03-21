use ratatui::style::Style;
use ratatui::text::Span;

use crate::ui::theme::Theme;

pub fn text_cursor_spans(
    content: &str,
    cursor: usize,
    viewport_offset: usize,
    visible_width: usize,
) -> Vec<Span<'static>> {
    if visible_width == 0 {
        return vec![];
    }

    let chars: Vec<char> = content.chars().collect();
    let total = chars.len();

    // Clamp viewport_offset to total length
    let vp = viewport_offset.min(total);

    // Determine how many chars are visible within the viewport
    let view_end = vp.saturating_add(visible_width).min(total);
    let visible: Vec<char> = chars[vp..view_end].to_vec();
    let cursor_in_view = cursor.saturating_sub(vp);

    // Block cursor: thin bar (▏) occupies a full cell and shifts text right, so we use bg/fg inversion instead.
    let cursor_style = Style::default()
        .bg(Theme::CURSOR_FG)
        .fg(Theme::SELECTION_BG);

    if cursor >= total {
        let text: String = visible.iter().collect();
        vec![Span::raw(text), Span::styled(" ", cursor_style)]
    } else if cursor_in_view < visible.len() {
        let before: String = visible[..cursor_in_view].iter().collect();
        let cursor_char: String = visible[cursor_in_view].to_string();
        let after: String = visible[cursor_in_view + 1..].iter().collect();
        vec![
            Span::raw(before),
            Span::styled(cursor_char, cursor_style),
            Span::raw(after),
        ]
    } else {
        // Cursor outside visible window (fallback): just show text
        let text: String = visible.iter().collect();
        vec![Span::raw(text)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_to_strings(spans: &[Span<'_>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn cursor_at_beginning() {
        let spans = text_cursor_spans("abc", 0, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", "a", "bc"]);
    }

    #[test]
    fn cursor_at_middle() {
        let spans = text_cursor_spans("abc", 1, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["a", "b", "c"]);
    }

    #[test]
    fn cursor_at_end() {
        let spans = text_cursor_spans("abc", 3, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["abc", " "]);
    }

    #[test]
    fn empty_string() {
        let spans = text_cursor_spans("", 0, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", " "]);
    }

    #[test]
    fn multibyte_characters() {
        let spans = text_cursor_spans("あいう", 1, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["あ", "い", "う"]);
    }

    #[test]
    fn viewport_offset_positive() {
        let spans = text_cursor_spans("abcdef", 3, 2, 3);

        // visible: "cde" (offset=2, width=3), cursor_in_view = 3-2 = 1
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["c", "d", "e"]);
    }

    #[test]
    fn viewport_offset_beyond_text_length() {
        let spans = text_cursor_spans("abc", 3, 10, 5);

        // vp clamped to 3 (total), visible is empty, cursor at end
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", " "]);
    }

    #[test]
    fn visible_width_one() {
        let spans = text_cursor_spans("abc", 1, 1, 1);

        // visible: "b", cursor_in_view = 0
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", "b", ""]);
    }

    #[test]
    fn visible_width_zero() {
        let spans = text_cursor_spans("abc", 1, 0, 0);

        assert!(spans.is_empty());
    }

    #[test]
    fn visible_width_usize_max_sentinel() {
        let spans = text_cursor_spans("hello", 2, 0, usize::MAX);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["he", "l", "lo"]);
    }

    #[test]
    fn cursor_style_is_consistent_across_positions() {
        let at_start = text_cursor_spans("abc", 0, 0, usize::MAX);
        let at_middle = text_cursor_spans("abc", 1, 0, usize::MAX);
        let at_end = text_cursor_spans("abc", 3, 0, usize::MAX);

        let cursor_start = &at_start[1];
        let cursor_middle = &at_middle[1];
        let cursor_end = at_end.last().unwrap();

        assert_eq!(cursor_start.style, cursor_middle.style);
        assert_eq!(cursor_middle.style, cursor_end.style);
    }
}
