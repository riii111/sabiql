use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::policy::sql::lexer::{SqlLexer, TokenKind};
use crate::ui::primitives::atoms::insert_cursor_span;
use crate::ui::theme::ThemePalette;

pub fn highlight_sql(text: &str, theme: &ThemePalette) -> Vec<Line<'static>> {
    highlight_sql_spans(text, theme)
        .into_iter()
        .map(Line::from)
        .collect()
}

pub fn highlight_sql_with_cursor(
    text: &str,
    cursor_row: usize,
    cursor_col: usize,
    theme: &ThemePalette,
) -> Vec<Line<'static>> {
    let mut lines = highlight_sql_spans(text, theme);
    if let Some(line) = lines.get_mut(cursor_row) {
        let spans = std::mem::take(line);
        *line = insert_cursor_span(spans, cursor_col, theme);
    }

    lines.into_iter().map(Line::from).collect()
}

fn highlight_sql_spans(text: &str, theme: &ThemePalette) -> Vec<Vec<Span<'static>>> {
    if text.is_empty() {
        return vec![];
    }

    let lexer = SqlLexer::new();
    let tokens = lexer.tokenize(text, text.chars().count());
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];

    for token in tokens {
        let style = token_style(&token.kind, theme);
        let mut segment = String::new();

        for ch in token.text.chars() {
            if ch == '\n' {
                if !segment.is_empty() {
                    lines
                        .last_mut()
                        .expect("sql highlight should always keep one line")
                        .push(Span::styled(std::mem::take(&mut segment), style));
                }
                lines.push(Vec::new());
            } else {
                segment.push(ch);
            }
        }

        if !segment.is_empty() {
            lines
                .last_mut()
                .expect("sql highlight should always keep one line")
                .push(Span::styled(segment, style));
        }
    }

    // Drop the trailing empty line so the line count matches `str::lines()`.
    // The editor appends a cursor-only line separately when the text ends with '\n'.
    if text.ends_with('\n') {
        lines.pop();
    }

    lines
}

fn token_style(kind: &TokenKind, theme: &ThemePalette) -> Style {
    match kind {
        TokenKind::Keyword(_) => Style::default()
            .fg(theme.sql_keyword)
            .add_modifier(Modifier::BOLD),
        TokenKind::StringLiteral => Style::default().fg(theme.sql_string),
        TokenKind::Number => Style::default().fg(theme.sql_number),
        TokenKind::Comment => Style::default().fg(theme.sql_comment),
        TokenKind::Operator(_) => Style::default().fg(theme.sql_operator),
        TokenKind::Identifier(_)
        | TokenKind::Punctuation(_)
        | TokenKind::Whitespace
        | TokenKind::Unknown => Style::default().fg(theme.sql_text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{DEFAULT_THEME, ThemePalette};

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn highlight_sql_splits_multiline_comment_across_lines() {
        let lines = highlight_sql("SELECT 1 /* hello\nworld */", &DEFAULT_THEME);

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "SELECT 1 /* hello");
        assert_eq!(line_text(&lines[1]), "world */");
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.style.fg == Some(DEFAULT_THEME.sql_comment))
        );
        assert!(
            lines[1]
                .spans
                .iter()
                .any(|span| span.style.fg == Some(DEFAULT_THEME.sql_comment))
        );
    }

    #[test]
    fn highlight_sql_marks_token_types_with_expected_colors() {
        let lines = highlight_sql("SELECT 'x', 42 -- note", &DEFAULT_THEME);
        let spans = &lines[0].spans;

        assert_eq!(spans[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].style.fg, Some(DEFAULT_THEME.sql_string));
        assert_eq!(spans[5].style.fg, Some(DEFAULT_THEME.sql_number));
        assert_eq!(spans[7].style.fg, Some(DEFAULT_THEME.sql_comment));
    }

    #[test]
    fn highlight_sql_with_cursor_preserves_neighbor_styles() {
        let lines = highlight_sql_with_cursor("SELECT 'x'", 0, 8, &DEFAULT_THEME);
        let spans = &lines[0].spans;

        assert_eq!(spans[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
        assert_eq!(spans[2].style.fg, Some(DEFAULT_THEME.sql_string));
        assert_eq!(spans[3].style.bg, Some(DEFAULT_THEME.cursor_bg));
        assert_eq!(spans[3].style.fg, Some(DEFAULT_THEME.cursor_text_fg));
        assert_eq!(spans[4].style.fg, Some(DEFAULT_THEME.sql_string));
    }

    #[test]
    fn highlight_sql_with_cursor_on_empty_middle_line_adds_cursor_cell() {
        let lines = highlight_sql_with_cursor("SELECT 1\n\nFROM users", 1, 0, &DEFAULT_THEME);

        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[1]), " ");
        assert_eq!(lines[1].spans[0].style.bg, Some(DEFAULT_THEME.cursor_bg));
    }

    #[test]
    fn highlight_sql_with_cursor_at_line_end_appends_cursor_cell() {
        let lines = highlight_sql_with_cursor("SELECT", 0, 6, &DEFAULT_THEME);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), "SELECT ");
        assert_eq!(
            spans.last().unwrap().style.bg,
            Some(DEFAULT_THEME.cursor_bg)
        );
        assert_eq!(spans[0].style.fg, Some(DEFAULT_THEME.sql_keyword));
    }

    #[test]
    fn highlight_sql_with_cursor_marks_double_quote_at_token_start() {
        let lines = highlight_sql_with_cursor(r#"SET "email" = 0"#, 0, 4, &DEFAULT_THEME);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), r#"SET "email" = 0"#);
        assert_eq!(spans[2].content.as_ref(), "\"");
        assert_eq!(spans[2].style.bg, Some(DEFAULT_THEME.cursor_bg));
        assert_eq!(spans[3].content.as_ref(), "email\"");
    }

    #[test]
    fn highlight_sql_with_cursor_marks_number_at_token_start() {
        let lines = highlight_sql_with_cursor(r#"SET "email" = 0"#, 0, 14, &DEFAULT_THEME);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), r#"SET "email" = 0"#);
        let number_span = spans
            .iter()
            .find(|span| span.content.as_ref() == "0")
            .expect("number token should be present");
        assert_eq!(number_span.style.bg, Some(DEFAULT_THEME.cursor_bg));
    }

    #[test]
    fn highlight_sql_honors_injected_theme_colors() {
        let custom_theme = ThemePalette {
            sql_keyword: ratatui::style::Color::Rgb(0x12, 0x34, 0x56),
            cursor_bg: ratatui::style::Color::Rgb(0xab, 0xcd, 0xef),
            ..DEFAULT_THEME
        };

        let highlighted = highlight_sql("SELECT", &custom_theme);
        let highlighted_with_cursor = highlight_sql_with_cursor("SELECT", 0, 0, &custom_theme);

        assert_eq!(
            highlighted[0].spans[0].style.fg,
            Some(custom_theme.sql_keyword)
        );
        assert_eq!(
            highlighted_with_cursor[0].spans[0].style.bg,
            Some(custom_theme.cursor_bg)
        );
    }
}
