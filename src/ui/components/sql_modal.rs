use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::state::{AppState, QueryState, SqlModalState};

use super::overlay::centered_rect;

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(80),
            Constraint::Percentage(60),
        );

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" SQL Editor ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into editor area and status line
        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        // Render the SQL editor content with cursor
        Self::render_editor(frame, editor_area, state);

        // Render status line
        Self::render_status(frame, status_area, state);
    }

    fn render_editor(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = &state.sql_modal_content;
        let cursor_pos = state.sql_modal_cursor;

        // Convert cursor position to (row, col)
        let (cursor_row, cursor_col) = Self::cursor_to_position(content, cursor_pos);

        // Build lines with cursor visualization
        let mut lines: Vec<Line> = if content.is_empty() {
            // Show placeholder with cursor
            vec![Line::from(vec![
                Span::styled("█", Style::default().fg(Color::White)),
                Span::styled(
                    " Enter SQL query...",
                    Style::default().fg(Color::DarkGray),
                ),
            ])]
        } else {
            content
                .lines()
                .enumerate()
                .map(|(row, line)| {
                    if row == cursor_row {
                        Self::line_with_cursor(line, cursor_col)
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect()
        };

        // If content ends with newline, add cursor on new line
        if content.ends_with('\n') && cursor_row == content.lines().count() {
            lines.push(Line::from(vec![Span::styled(
                "█",
                Style::default().fg(Color::White),
            )]));
        }

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        frame.render_widget(paragraph, area);
    }

    fn line_with_cursor(line: &str, cursor_col: usize) -> Line<'static> {
        let chars: Vec<char> = line.chars().collect();

        if cursor_col >= chars.len() {
            // Cursor at end of line
            let mut spans = vec![Span::raw(line.to_string())];
            spans.push(Span::styled("█", Style::default().fg(Color::White)));
            Line::from(spans)
        } else {
            // Cursor in middle of line
            let before: String = chars[..cursor_col].iter().collect();
            let cursor_char: String = chars[cursor_col].to_string();
            let after: String = chars[cursor_col + 1..].iter().collect();

            Line::from(vec![
                Span::raw(before),
                Span::styled(
                    cursor_char,
                    Style::default()
                        .bg(Color::White)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(after),
            ])
        }
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState) {
        let status_text = match state.sql_modal_state {
            SqlModalState::Editing => {
                if state.query_state == QueryState::Running {
                    "Running..."
                } else {
                    "Ready"
                }
            }
            SqlModalState::Running => "Running...",
            SqlModalState::Success => "OK",
            SqlModalState::Error => "Error",
        };

        let status_style = match state.sql_modal_state {
            SqlModalState::Editing => {
                if state.query_state == QueryState::Running {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                }
            }
            SqlModalState::Running => Style::default().fg(Color::Yellow),
            SqlModalState::Success => Style::default().fg(Color::Green),
            SqlModalState::Error => Style::default().fg(Color::Red),
        };

        let hints = " Ctrl+Enter: Run  Esc: Close";

        let line = Line::from(vec![
            Span::styled(status_text, status_style),
            Span::raw(" │"),
            Span::styled(hints, Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        frame.render_widget(paragraph, area);
    }

    /// Convert a character index to (row, col) position
    fn cursor_to_position(content: &str, cursor_pos: usize) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;
        let mut current_pos = 0;

        for ch in content.chars() {
            if current_pos >= cursor_pos {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
            current_pos += 1;
        }

        (row, col)
    }

    /// Convert (row, col) position to character index
    #[allow(dead_code)]
    pub fn position_to_cursor(content: &str, row: usize, col: usize) -> usize {
        let mut current_row = 0;
        let mut current_col = 0;
        let mut cursor_pos = 0;

        for ch in content.chars() {
            if current_row == row && current_col == col {
                return cursor_pos;
            }
            if ch == '\n' {
                if current_row == row {
                    // End of target row, return position at end of line
                    return cursor_pos;
                }
                current_row += 1;
                current_col = 0;
            } else {
                current_col += 1;
            }
            cursor_pos += 1;
        }

        cursor_pos
    }

    /// Get the line lengths for cursor movement
    #[allow(dead_code)]
    pub fn line_lengths(content: &str) -> Vec<usize> {
        content.lines().map(|l| l.chars().count()).collect()
    }
}
