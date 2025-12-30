use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::app::state::{AppState, CompletionKind, QueryState, SqlModalState};
use crate::ui::theme::Theme;

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(80),
            Constraint::Percentage(60),
        );

        render_scrim(frame);
        frame.render_widget(Clear, area);

        let block = modal_block_with_hint(
            " SQL Editor ".to_string(),
            " Alt+Enter: Run │ Ctrl+L: Clear │ Esc: Close".to_string(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into editor area and status line
        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        // Render the SQL editor content with cursor
        Self::render_editor(frame, editor_area, state);

        // Render status line
        Self::render_status(frame, status_area, state);

        // Render completion popup if visible
        if state.completion.visible && !state.completion.candidates.is_empty() {
            Self::render_completion_popup(frame, area, editor_area, state);
        }
    }

    fn render_editor(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = &state.sql_modal_content;
        let cursor_pos = state.sql_modal_cursor;

        // Convert cursor position to (row, col)
        let (cursor_row, cursor_col) = Self::cursor_to_position(content, cursor_pos);

        let current_line_style = Style::default().bg(Theme::EDITOR_CURRENT_LINE_BG);

        // Build lines with cursor visualization and current line highlight
        let mut lines: Vec<Line> = if content.is_empty() {
            // Show placeholder with cursor (highlighted)
            vec![
                Line::from(vec![
                    Span::styled("█", Style::default().fg(Color::White)),
                    Span::styled(" Enter SQL query...", Style::default().fg(Color::DarkGray)),
                ])
                .style(current_line_style),
            ]
        } else {
            content
                .lines()
                .enumerate()
                .map(|(row, line)| {
                    if row == cursor_row {
                        Self::line_with_cursor(line, cursor_col).style(current_line_style)
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect()
        };

        // If content ends with newline, add cursor on new line (highlighted)
        if content.ends_with('\n') && cursor_row == content.lines().count() {
            lines.push(
                Line::from(vec![Span::styled("█", Style::default().fg(Color::White))])
                    .style(current_line_style),
            );
        }

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Theme::MODAL_BG));

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
        let is_running = state.query_state == QueryState::Running;

        let (status_text, status_style) = if is_running {
            let spinner_frames = ["◐", "◓", "◑", "◒"];
            let elapsed = state
                .query_start_time
                .map(|t| t.elapsed())
                .unwrap_or_default();
            let frame_idx = (elapsed.as_millis() / 300) as usize % spinner_frames.len();
            let spinner = spinner_frames[frame_idx];
            let elapsed_secs = elapsed.as_secs_f32();
            let status = format!("{} Running {:.1}s", spinner, elapsed_secs);
            (status, Style::default().fg(Color::Yellow))
        } else {
            match state.sql_modal_state {
                SqlModalState::Editing => {
                    ("Ready".to_string(), Style::default().fg(Color::DarkGray))
                }
                SqlModalState::Running => {
                    ("Running...".to_string(), Style::default().fg(Color::Yellow))
                }
                SqlModalState::Success => ("OK".to_string(), Style::default().fg(Color::Green)),
                SqlModalState::Error => ("Error".to_string(), Style::default().fg(Color::Red)),
            }
        };

        let hints = " Alt+Enter: Run  Ctrl+L: Clear  Esc: Close";

        let line = Line::from(vec![
            Span::styled(status_text, status_style),
            Span::raw(" │"),
            Span::styled(hints, Style::default().fg(Theme::MODAL_HINT)),
        ]);

        let paragraph = Paragraph::new(line).style(Style::default().bg(Theme::MODAL_BG));

        frame.render_widget(paragraph, area);
    }

    fn render_completion_popup(
        frame: &mut Frame,
        modal_area: Rect,
        editor_area: Rect,
        state: &AppState,
    ) {
        let (cursor_row, cursor_col) =
            Self::cursor_to_position(&state.sql_modal_content, state.sql_modal_cursor);

        // Popup dimensions
        let max_items = 8;
        let visible_count = state.completion.candidates.len().min(max_items);
        let popup_height = (visible_count as u16) + 2; // +2 for borders
        let popup_width = 45u16.min(modal_area.width);

        // Position popup below cursor (global coordinates)
        // Ensure popup fits within modal bounds
        let popup_x = if modal_area.width < popup_width {
            modal_area.x
        } else {
            (editor_area.x + cursor_col as u16).min(modal_area.right().saturating_sub(popup_width))
        };
        let cursor_screen_y = editor_area.y + cursor_row as u16;

        // Show above cursor if not enough space below
        let popup_y = if cursor_screen_y + 1 + popup_height > modal_area.bottom() {
            cursor_screen_y.saturating_sub(popup_height)
        } else {
            cursor_screen_y + 1
        };

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        frame.render_widget(Clear, popup_area);

        // Calculate scroll window to keep selected item visible
        let selected = state.completion.selected_index;
        let total = state.completion.candidates.len();
        let scroll_offset = if total <= max_items {
            0
        } else {
            // Keep selected item in middle of window when possible
            let half = max_items / 2;
            if selected < half {
                0
            } else if selected >= total - half {
                total - max_items
            } else {
                selected - half
            }
        };

        // Calculate max text width for alignment
        let max_text_width = state
            .completion
            .candidates
            .iter()
            .skip(scroll_offset)
            .take(max_items)
            .map(|c| c.text.len())
            .max()
            .unwrap_or(0);

        let items: Vec<ListItem> = state
            .completion
            .candidates
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_items)
            .map(|(i, candidate)| {
                let is_selected = i == selected;

                // Kind label (pgcli style)
                let kind_label = match candidate.kind {
                    CompletionKind::Keyword => "keyword",
                    CompletionKind::Schema => "schema",
                    CompletionKind::Table => "table",
                    CompletionKind::Column => "column",
                };

                // Format: "text    kind" with padding for alignment
                let padding = max_text_width.saturating_sub(candidate.text.len()) + 2;
                let text = format!(
                    " {}{:padding$}{}",
                    candidate.text,
                    "",
                    kind_label,
                    padding = padding
                );

                let style = if is_selected {
                    Style::default()
                        .bg(Theme::COMPLETION_SELECTED_BG)
                        .fg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };

                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default().bg(Theme::MODAL_BG)),
        );

        frame.render_widget(list, popup_area);
    }

    /// Convert a character index to (row, col) position
    /// NOTE: This is O(n) on every render. For long documents, consider caching
    /// line offsets and only recalculating on content change.
    fn cursor_to_position(content: &str, cursor_pos: usize) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;

        for (current_pos, ch) in content.chars().enumerate() {
            if current_pos >= cursor_pos {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn empty_string_returns_zero_position() {
        let result = SqlModal::cursor_to_position("", 0);

        assert_eq!(result, (0, 0));
    }

    #[test]
    fn single_line_ascii_returns_correct_position() {
        let content = "SELECT * FROM users";

        let result = SqlModal::cursor_to_position(content, 7);

        assert_eq!(result, (0, 7));
    }

    #[test]
    fn multiple_lines_returns_correct_row_and_col() {
        let content = "SELECT *\nFROM users\nWHERE id = 1";

        let result = SqlModal::cursor_to_position(content, 17);

        assert_eq!(result, (1, 8));
    }

    #[test]
    fn multibyte_characters_count_correctly() {
        let content = "こんにちは世界";

        let result = SqlModal::cursor_to_position(content, 3);

        assert_eq!(result, (0, 3));
    }

    #[rstest]
    #[case("SELECT 日本語", 7, (0, 7))]
    #[case("SELECT 日本語", 8, (0, 8))]
    #[case("SELECT 日本語", 9, (0, 9))]
    #[case("こんにちは\n世界", 5, (0, 5))]
    #[case("こんにちは\n世界", 6, (1, 0))]
    #[case("こんにちは\n世界", 7, (1, 1))]
    fn multibyte_cursor_positions_are_accurate(
        #[case] input: &str,
        #[case] cursor: usize,
        #[case] expected: (usize, usize),
    ) {
        let result = SqlModal::cursor_to_position(input, cursor);

        assert_eq!(result, expected);
    }

    #[test]
    fn position_to_cursor_converts_back_correctly() {
        let content = "SELECT *\nFROM users";

        let cursor = SqlModal::position_to_cursor(content, 1, 5);

        assert_eq!(cursor, 14);
    }

    #[test]
    fn position_to_cursor_with_multibyte_returns_correct_index() {
        let content = "こんにちは\n世界";

        let cursor = SqlModal::position_to_cursor(content, 1, 2);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn cursor_at_end_of_line_returns_line_length() {
        let content = "SELECT *\nFROM users";

        let cursor = SqlModal::position_to_cursor(content, 0, 100);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn line_lengths_counts_chars_not_bytes() {
        let content = "abc\n日本語\nxyz";

        let lengths = SqlModal::line_lengths(content);

        assert_eq!(lengths, vec![3, 3, 3]);
    }

    #[rstest]
    #[case("", vec![])]
    #[case("single", vec![6])]
    #[case("one\ntwo", vec![3, 3])]
    #[case("あ\nい\nう", vec![1, 1, 1])]
    fn line_lengths_handles_various_inputs(#[case] input: &str, #[case] expected: Vec<usize>) {
        let result = SqlModal::line_lengths(input);

        assert_eq!(result, expected);
    }
}
