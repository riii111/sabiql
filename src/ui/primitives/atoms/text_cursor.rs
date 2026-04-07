use std::cmp::Ordering;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::ui::theme::ThemePalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Block,
    Insert,
}

impl CursorKind {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Block => " ",
            Self::Insert => "\u{258f}",
        }
    }
}

pub fn cursor_style(theme: &ThemePalette) -> Style {
    cursor_style_for(CursorKind::Block, theme)
}

pub fn cursor_style_for(kind: CursorKind, theme: &ThemePalette) -> Style {
    match kind {
        CursorKind::Block => theme.block_cursor_style(),
        CursorKind::Insert => theme.insert_cursor_style(),
    }
}

pub fn text_cursor_spans(
    content: &str,
    cursor: usize,
    viewport_offset: usize,
    visible_width: usize,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    text_cursor_spans_with_kind(
        content,
        cursor,
        viewport_offset,
        visible_width,
        CursorKind::Block,
        theme,
    )
}

pub fn text_cursor_spans_with_kind(
    content: &str,
    cursor: usize,
    viewport_offset: usize,
    visible_width: usize,
    kind: CursorKind,
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

    let cursor_style = cursor_style_for(kind, theme);

    match kind {
        CursorKind::Block => {
            if cursor >= total {
                let text: String = visible.iter().collect();
                vec![Span::raw(text), Span::styled(kind.glyph(), cursor_style)]
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
                let text: String = visible.iter().collect();
                vec![Span::raw(text)]
            }
        }
        CursorKind::Insert => {
            let _ = cursor_in_view;
            let _ = cursor_style;
            let text: String = visible.iter().collect();
            vec![Span::raw(text)]
        }
    }
}

pub fn insert_cursor_span(
    spans: Vec<Span<'static>>,
    cursor_col: usize,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    insert_cursor_span_with_kind(spans, cursor_col, CursorKind::Block, theme)
}

pub fn insert_cursor_span_with_kind(
    spans: Vec<Span<'static>>,
    cursor_col: usize,
    kind: CursorKind,
    theme: &ThemePalette,
) -> Vec<Span<'static>> {
    let cursor_style = cursor_style_for(kind, theme);
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
            if iter.peek().is_none() && matches!(kind, CursorKind::Block) {
                output.push(Span::styled(kind.glyph(), cursor_style));
                return output;
            }

            if iter.peek().is_none() {
                return output;
            }

            if matches!(kind, CursorKind::Insert) {
                output.extend(iter);
                return output;
            }

            remaining = 0;
            continue;
        }

        let (before, current, after) = split_at_cursor(content, remaining);
        if !before.is_empty() {
            output.push(Span::styled(before, span.style));
        }

        match kind {
            CursorKind::Block => {
                output.push(Span::styled(current, span.style.patch(cursor_style)));
                if !after.is_empty() {
                    output.push(Span::styled(after, span.style));
                }
            }
            CursorKind::Insert => {
                let mut remainder = current;
                remainder.push_str(&after);
                if !remainder.is_empty() {
                    output.push(Span::styled(remainder, span.style));
                }
                output.extend(iter);
                return output;
            }
        }
        output.extend(iter);
        return output;
    }

    if matches!(kind, CursorKind::Block) {
        output.push(Span::styled(kind.glyph(), cursor_style));
    }
    output
}

pub fn set_terminal_cursor(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    x_offset: u16,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let available_width = area.width.saturating_sub(x_offset) as usize;
    if available_width == 0 {
        return;
    }

    let Some((visible_row, visible_col)) =
        visual_cursor_position(content, cursor_row, cursor_col, scroll_row, available_width)
    else {
        return;
    };
    if visible_row >= area.height as usize {
        return;
    }

    let x = area
        .x
        .saturating_add(x_offset)
        .saturating_add(visible_col as u16)
        .min(area.right().saturating_sub(1));
    let y = area.y.saturating_add(visible_row as u16);

    frame.set_cursor_position((x, y));
}

#[derive(Debug, Clone, Copy)]
pub struct ModalTextSurface<'a> {
    pub content: &'a str,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub cursor_kind: CursorKind,
    pub empty_placeholder: &'a str,
    pub base_style: Style,
    pub current_line_style: Style,
}

pub fn build_modal_text_surface_lines(
    surface: ModalTextSurface<'_>,
    mut line_spans: Vec<Vec<Span<'static>>>,
    theme: &ThemePalette,
) -> Vec<Line<'static>> {
    let placeholder_span = placeholder_span(surface.cursor_kind, theme);

    let mut lines = if surface.content.is_empty() {
        vec![
            Line::from(vec![
                placeholder_span.clone(),
                Span::styled(
                    surface.empty_placeholder.to_string(),
                    Style::default().fg(theme.placeholder_text),
                ),
            ])
            .style(surface.current_line_style),
        ]
    } else {
        if let Some(line) = line_spans.get_mut(surface.cursor_row) {
            let spans = std::mem::take(line);
            *line =
                insert_cursor_span_with_kind(spans, surface.cursor_col, surface.cursor_kind, theme);
        }

        line_spans
            .into_iter()
            .enumerate()
            .map(|(row, spans)| {
                if row == surface.cursor_row {
                    Line::from(spans).style(surface.current_line_style)
                } else {
                    Line::from(spans)
                }
            })
            .collect()
    };

    if surface.content.ends_with('\n') && surface.cursor_row == surface.content.lines().count() {
        lines.push(Line::from(vec![placeholder_span]).style(surface.current_line_style));
    }

    lines
}

pub fn render_modal_text_surface(
    frame: &mut Frame,
    area: Rect,
    surface: ModalTextSurface<'_>,
    lines: Vec<Line<'static>>,
) {
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((surface.scroll_row as u16, 0))
            .style(surface.base_style),
        area,
    );

    if matches!(surface.cursor_kind, CursorKind::Insert) {
        set_terminal_cursor(
            frame,
            area,
            surface.content,
            surface.cursor_row,
            surface.cursor_col,
            surface.scroll_row,
            0,
        );
    }
}

fn display_width_up_to_char(text: &str, cursor_col: usize) -> u16 {
    let byte_idx = text
        .char_indices()
        .nth(cursor_col)
        .map_or(text.len(), |(idx, _)| idx);
    UnicodeWidthStr::width(&text[..byte_idx]).min(u16::MAX as usize) as u16
}

fn placeholder_span(kind: CursorKind, theme: &ThemePalette) -> Span<'static> {
    match kind {
        CursorKind::Block => Span::styled(kind.glyph(), cursor_style_for(kind, theme)),
        CursorKind::Insert => Span::raw(" "),
    }
}

fn visual_cursor_position(
    content: &str,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    available_width: usize,
) -> Option<(usize, usize)> {
    if available_width == 0 || cursor_row < scroll_row {
        return None;
    }

    let mut wrapped_rows_before_cursor = 0;
    let mut current_line = "";

    for (row, line) in content.split('\n').enumerate() {
        if row < scroll_row {
            continue;
        }

        match row.cmp(&cursor_row) {
            Ordering::Less => {
                wrapped_rows_before_cursor += wrapped_visual_rows(line, available_width);
            }
            Ordering::Equal => {
                current_line = line;
                break;
            }
            Ordering::Greater => return None,
        }
    }

    let cursor_display_width = display_width_up_to_char(current_line, cursor_col) as usize;

    Some((
        wrapped_rows_before_cursor + cursor_display_width / available_width,
        cursor_display_width % available_width,
    ))
}

fn wrapped_visual_rows(line: &str, available_width: usize) -> usize {
    UnicodeWidthStr::width(line)
        .max(1)
        .div_ceil(available_width)
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
    fn display_width_uses_terminal_cell_width() {
        assert_eq!(display_width_up_to_char("a語b", 0), 0);
        assert_eq!(display_width_up_to_char("a語b", 1), 1);
        assert_eq!(display_width_up_to_char("a語b", 2), 3);
        assert_eq!(display_width_up_to_char("a語b", 3), 4);
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
    fn insert_mode_cursor_preserves_text_without_glyph() {
        let spans = text_cursor_spans_with_kind(
            "abc",
            1,
            0,
            usize::MAX,
            CursorKind::Insert,
            &DEFAULT_THEME,
        );

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["abc"]);
    }

    #[test]
    fn insert_mode_cursor_at_end_keeps_text_width() {
        let spans = text_cursor_spans_with_kind(
            "abc",
            3,
            0,
            usize::MAX,
            CursorKind::Insert,
            &DEFAULT_THEME,
        );

        let texts = spans_to_strings(&spans);
        assert_eq!(texts, vec!["abc"]);
    }

    #[test]
    fn visual_cursor_position_wraps_with_display_width() {
        assert_eq!(visual_cursor_position("a語b", 0, 2, 0, 2), Some((1, 1)));
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

    #[test]
    fn insert_cursor_span_with_insert_kind_preserves_text_without_glyph() {
        let spans = vec![Span::styled(
            "abcd".to_string(),
            Style::default().fg(DEFAULT_THEME.sql_keyword),
        )];

        let inserted = insert_cursor_span_with_kind(spans, 2, CursorKind::Insert, &DEFAULT_THEME);

        let texts: Vec<String> = inserted.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(texts, vec!["ab", "cd"]);
        assert_eq!(inserted[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
        assert_eq!(inserted[1].style.fg, Some(DEFAULT_THEME.sql_keyword));
    }
}
