use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::policy::sql::lexer::{SqlLexer, TokenKind};
use crate::ui::primitives::atoms::insert_cursor_span;
use crate::ui::theme::Theme;

pub fn highlight_sql(text: &str) -> Vec<Line<'static>> {
    highlight_sql_spans(text)
        .into_iter()
        .map(Line::from)
        .collect()
}

pub fn highlight_sql_with_cursor(
    text: &str,
    cursor_row: usize,
    cursor_col: usize,
) -> Vec<Line<'static>> {
    let mut lines = highlight_sql_spans(text);
    if let Some(line) = lines.get_mut(cursor_row) {
        let spans = std::mem::take(line);
        *line = insert_cursor_span(spans, cursor_col);
    }

    lines.into_iter().map(Line::from).collect()
}

fn highlight_sql_spans(text: &str) -> Vec<Vec<Span<'static>>> {
    if text.is_empty() {
        return vec![];
    }

    let lexer = SqlLexer::new();
    let tokens = lexer.tokenize(text, text.chars().count());
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];

    for token in tokens {
        let style = token_style(&token.kind);
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

fn token_style(kind: &TokenKind) -> Style {
    match kind {
        TokenKind::Keyword(_) => Style::default()
            .fg(Theme::SQL_KEYWORD)
            .add_modifier(Modifier::BOLD),
        TokenKind::StringLiteral => Style::default().fg(Theme::SQL_STRING),
        TokenKind::Number => Style::default().fg(Theme::SQL_NUMBER),
        TokenKind::Comment => Style::default().fg(Theme::SQL_COMMENT),
        TokenKind::Operator(_) => Style::default().fg(Theme::SQL_OPERATOR),
        TokenKind::Identifier(_)
        | TokenKind::Punctuation(_)
        | TokenKind::Whitespace
        | TokenKind::Unknown => Style::default().fg(Theme::SQL_TEXT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn highlight_sql_splits_multiline_comment_across_lines() {
        let lines = highlight_sql("SELECT 1 /* hello\nworld */");

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "SELECT 1 /* hello");
        assert_eq!(line_text(&lines[1]), "world */");
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.style.fg == Some(Theme::SQL_COMMENT))
        );
        assert!(
            lines[1]
                .spans
                .iter()
                .any(|span| span.style.fg == Some(Theme::SQL_COMMENT))
        );
    }

    #[test]
    fn highlight_sql_marks_token_types_with_expected_colors() {
        let lines = highlight_sql("SELECT 'x', 42 -- note");
        let spans = &lines[0].spans;

        assert_eq!(spans[0].style.fg, Some(Theme::SQL_KEYWORD));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].style.fg, Some(Theme::SQL_STRING));
        assert_eq!(spans[5].style.fg, Some(Theme::SQL_NUMBER));
        assert_eq!(spans[7].style.fg, Some(Theme::SQL_COMMENT));
    }

    #[test]
    fn highlight_sql_with_cursor_preserves_neighbor_styles() {
        let lines = highlight_sql_with_cursor("SELECT 'x'", 0, 8);
        let spans = &lines[0].spans;

        assert_eq!(spans[0].style.fg, Some(Theme::SQL_KEYWORD));
        assert_eq!(spans[2].style.fg, Some(Theme::SQL_STRING));
        assert_eq!(spans[3].style.bg, Some(Theme::CURSOR_FG));
        assert_eq!(spans[3].style.fg, Some(Theme::SELECTION_BG));
        assert_eq!(spans[4].style.fg, Some(Theme::SQL_STRING));
    }

    #[test]
    fn highlight_sql_with_cursor_on_empty_middle_line_adds_cursor_cell() {
        let lines = highlight_sql_with_cursor("SELECT 1\n\nFROM users", 1, 0);

        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[1]), " ");
        assert_eq!(lines[1].spans[0].style.bg, Some(Theme::CURSOR_FG));
    }

    #[test]
    fn highlight_sql_with_cursor_at_line_end_appends_cursor_cell() {
        let lines = highlight_sql_with_cursor("SELECT", 0, 6);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), "SELECT ");
        assert_eq!(spans.last().unwrap().style.bg, Some(Theme::CURSOR_FG));
        assert_eq!(spans[0].style.fg, Some(Theme::SQL_KEYWORD));
    }

    #[test]
    fn highlight_sql_with_cursor_marks_double_quote_at_token_start() {
        let lines = highlight_sql_with_cursor(r#"SET "email" = 0"#, 0, 4);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), r#"SET "email" = 0"#);
        assert_eq!(spans[2].content.as_ref(), "\"");
        assert_eq!(spans[2].style.bg, Some(Theme::CURSOR_FG));
        assert_eq!(spans[3].content.as_ref(), "email\"");
    }

    #[test]
    fn highlight_sql_with_cursor_marks_number_at_token_start() {
        let lines = highlight_sql_with_cursor(r#"SET "email" = 0"#, 0, 14);
        let spans = &lines[0].spans;

        assert_eq!(line_text(&lines[0]), r#"SET "email" = 0"#);
        let number_span = spans
            .iter()
            .find(|span| span.content.as_ref() == "0")
            .expect("number token should be present");
        assert_eq!(number_span.style.bg, Some(Theme::CURSOR_FG));
    }
}
