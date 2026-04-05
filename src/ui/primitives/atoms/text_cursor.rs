use std::cmp::Ordering;

use ratatui::style::Style;
use ratatui::text::Span;

use crate::ui::theme::ThemePalette;

pub fn cursor_style(theme: &ThemePalette) -> Style {
    theme.cursor_style()
}

pub fn text_cursor_spans(
    content: &str,
    cursor: usize,
    viewport_offset: usize,
    visible_width: usize,
    theme: &ThemePalette,
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
    let cursor_in_view = cursor.checked_sub(vp);

    // Block cursor: thin bar (▏) occupies a full cell and shifts text right, so we use bg/fg inversion instead.
    let cursor_style = cursor_style(theme);

    if cursor >= total {
        let text: String = visible.iter().collect();
        vec![Span::raw(text), Span::styled(" ", cursor_style)]
    } else if let Some(ci) = cursor_in_view.filter(|&i| i < visible.len()) {
        let before: String = visible[..ci].iter().collect();
        let cursor_char: String = visible[ci].to_string();
        let after: String = visible[ci + 1..].iter().collect();
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

pub fn insert_cursor_span(
    spans: Vec<Span<'static>>,
    cursor_col: usize,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    let mut output = Vec::new();
    let mut remaining = cursor_col;
    let mut iter = spans.into_iter().peekable();

    while let Some(span) = iter.next() {
        let content = span.content.as_ref();
        let len = content.chars().count();

        if remaining > len {
            remaining -= len;
            output.push(span);
            continue;
        }

        if remaining == len {
            output.push(span);
            if iter.peek().is_none() {
                output.push(Span::styled(" ", cursor_style(theme)));
                return output;
            }

            remaining = 0;
            continue;
        }

        let (before, current, after) = split_at_cursor(content, remaining);
        if !before.is_empty() {
            output.push(Span::styled(before, span.style));
        }
        output.push(Span::styled(current, span.style.patch(cursor_style(theme))));
        if !after.is_empty() {
            output.push(Span::styled(after, span.style));
        }
        output.extend(iter);
        return output;
    }

    output.push(Span::styled(" ", cursor_style(theme)));
    output
}

fn split_at_cursor(text: &str, cursor_col: usize) -> (String, String, String) {
    let mut before = String::new();
    let mut current = String::new();
    let mut after = String::new();

    for (idx, ch) in text.chars().enumerate() {
        match idx.cmp(&cursor_col) {
            Ordering::Less => before.push(ch),
            Ordering::Equal => current.push(ch),
            Ordering::Greater => after.push(ch),
        }
    }

    (before, current, after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT_THEME;
    use ratatui::style::Modifier;

    fn spans_to_strings(spans: &[Span<'_>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn cursor_at_beginning() {
        let spans = text_cursor_spans("abc", 0, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", "a", "bc"]);
    }

    #[test]
    fn cursor_at_middle() {
        let spans = text_cursor_spans("abc", 1, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["a", "b", "c"]);
    }

    #[test]
    fn cursor_at_end() {
        let spans = text_cursor_spans("abc", 3, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["abc", " "]);
    }

    #[test]
    fn empty_string() {
        let spans = text_cursor_spans("", 0, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", " "]);
    }

    #[test]
    fn multibyte_characters() {
        let spans = text_cursor_spans("あいう", 1, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["あ", "い", "う"]);
    }

    #[test]
    fn viewport_offset_positive() {
        let spans = text_cursor_spans("abcdef", 3, 2, 3, &DEFAULT_THEME);

        // visible: "cde" (offset=2, width=3), cursor_in_view = 3-2 = 1
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["c", "d", "e"]);
    }

    #[test]
    fn viewport_offset_beyond_text_length() {
        let spans = text_cursor_spans("abc", 3, 10, 5, &DEFAULT_THEME);

        // vp clamped to 3 (total), visible is empty, cursor at end
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", " "]);
    }

    #[test]
    fn visible_width_one() {
        let spans = text_cursor_spans("abc", 1, 1, 1, &DEFAULT_THEME);

        // visible: "b", cursor_in_view = 0
        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["", "b", ""]);
    }

    #[test]
    fn visible_width_zero() {
        let spans = text_cursor_spans("abc", 1, 0, 0, &DEFAULT_THEME);

        assert!(spans.is_empty());
    }

    #[test]
    fn visible_width_usize_max_sentinel() {
        let spans = text_cursor_spans("hello", 2, 0, usize::MAX, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["he", "l", "lo"]);
    }

    #[test]
    fn cursor_left_of_viewport_returns_text_only() {
        // cursor=0, viewport starts at 2 -> cursor is off-screen to the left
        let spans = text_cursor_spans("abcdef", 0, 2, 3, &DEFAULT_THEME);

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["cde"]);
    }

    #[test]
    fn all_positions_return_consistent_cursor_style() {
        let at_start = text_cursor_spans("abc", 0, 0, usize::MAX, &DEFAULT_THEME);
        let at_middle = text_cursor_spans("abc", 1, 0, usize::MAX, &DEFAULT_THEME);
        let at_end = text_cursor_spans("abc", 3, 0, usize::MAX, &DEFAULT_THEME);

        let cursor_start = &at_start[1];
        let cursor_middle = &at_middle[1];
        let cursor_end = at_end.last().unwrap();

        assert_eq!(cursor_start.style, cursor_middle.style);
        assert_eq!(cursor_middle.style, cursor_end.style);
    }

    #[test]
    fn insert_cursor_span_preserves_existing_styles_across_boundary() {
        let spans = vec![
            Span::styled(
                "ab".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_keyword),
            ),
            Span::styled(
                "cd".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_string),
            ),
            Span::styled(
                "ef".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_comment),
            ),
        ];

        let inserted = insert_cursor_span(spans, 2, &DEFAULT_THEME);

        let texts: Vec<String> = inserted.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(texts, vec!["ab", "c", "d", "ef"]);
        assert_eq!(inserted[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
        assert_eq!(inserted[1].style, cursor_style(&DEFAULT_THEME));
        assert_eq!(inserted[2].style.fg, Some(DEFAULT_THEME.sql_string));
        assert_eq!(inserted[3].style.fg, Some(DEFAULT_THEME.sql_comment));
    }

    #[test]
    fn insert_cursor_span_uses_next_span_at_boundary() {
        let spans = vec![
            Span::styled(
                "ab".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_keyword),
            ),
            Span::styled(
                "cd".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_string),
            ),
        ];

        let inserted = insert_cursor_span(spans, 2, &DEFAULT_THEME);

        let texts: Vec<String> = inserted.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(texts, vec!["ab", "c", "d"]);
        assert_eq!(inserted[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
        assert_eq!(inserted[1].style, cursor_style(&DEFAULT_THEME));
        assert_eq!(inserted[2].style.fg, Some(DEFAULT_THEME.sql_string));
    }

    #[test]
    fn insert_cursor_span_preserves_modifiers_on_cursor_character() {
        let spans = vec![Span::styled(
            "ab".to_string(),
            Style::default()
                .fg(DEFAULT_THEME.sql_keyword)
                .add_modifier(Modifier::BOLD),
        )];

        let inserted = insert_cursor_span(spans, 0, &DEFAULT_THEME);

        assert_eq!(inserted[0].content.as_ref(), "a");
        assert_eq!(inserted[0].style.bg, Some(DEFAULT_THEME.cursor_bg));
        assert_eq!(inserted[0].style.fg, Some(DEFAULT_THEME.cursor_text_fg));
        assert!(inserted[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn insert_cursor_span_at_true_end_appends_cursor_cell() {
        let spans = vec![
            Span::styled(
                "ab".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_keyword),
            ),
            Span::styled(
                "cd".to_string(),
                Style::default().fg(DEFAULT_THEME.sql_string),
            ),
        ];

        let inserted = insert_cursor_span(spans, 4, &DEFAULT_THEME);

        let texts: Vec<String> = inserted.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(texts, vec!["ab", "cd", " "]);
        assert_eq!(inserted[2].style, cursor_style(&DEFAULT_THEME));
    }
}
