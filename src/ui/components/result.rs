use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;
use crate::domain::{QueryResult, QuerySource};

pub struct ResultPane;

impl ResultPane {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let is_focused = state.focused_pane == FocusedPane::Result;

        // Check if we should show highlight (flash effect on new results)
        let should_highlight = state
            .result_highlight_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else if should_highlight {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Determine which result to show
        let result = Self::current_result(state);

        // Build title with source badge
        let title = Self::build_title(result, state);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        if let Some(result) = result {
            if result.is_error() {
                Self::render_error(frame, area, result, block);
            } else if result.rows.is_empty() {
                Self::render_empty(frame, area, block);
            } else {
                Self::render_table(frame, area, result, block, state.result_scroll_offset);
            }
        } else {
            Self::render_placeholder(frame, area, block);
        }
    }

    fn current_result(state: &AppState) -> Option<&QueryResult> {
        match state.history_index {
            None => state.current_result.as_ref(),
            Some(i) => state.result_history.get(i),
        }
    }

    fn build_title(result: Option<&QueryResult>, state: &AppState) -> String {
        match result {
            None => " [3] Result ".to_string(),
            Some(r) => {
                let source_badge = match r.source {
                    QuerySource::Preview => "PREVIEW".to_string(),
                    QuerySource::Adhoc => {
                        if let Some(idx) = state.history_index {
                            format!("ADHOC #{}", idx + 1)
                        } else {
                            "ADHOC".to_string()
                        }
                    }
                };

                if r.is_error() {
                    format!(" [3] Result [{}] ERROR ", source_badge)
                } else {
                    format!(
                        " [3] Result [{}] ({}, {}ms) ",
                        source_badge,
                        r.row_count_display(),
                        r.execution_time_ms
                    )
                }
            }
        }
    }

    fn render_placeholder(frame: &mut Frame, area: Rect, block: Block) {
        let content = Paragraph::new("(select a table to preview)")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(content, area);
    }

    fn render_empty(frame: &mut Frame, area: Rect, block: Block) {
        let content = Paragraph::new("No rows returned")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(content, area);
    }

    fn render_error(frame: &mut Frame, area: Rect, result: &QueryResult, block: Block) {
        let error_msg = result.error.as_deref().unwrap_or("Unknown error");

        let block = block.style(Style::default().fg(Color::Red));

        let content = Paragraph::new(error_msg)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Red));

        frame.render_widget(content, area);
    }

    fn render_table(
        frame: &mut Frame,
        area: Rect,
        result: &QueryResult,
        block: Block,
        scroll_offset: usize,
    ) {
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if result.columns.is_empty() {
            return;
        }

        // Calculate column widths dynamically based on content and available space
        let widths = calculate_column_widths(&result.columns, &result.rows, inner.width);

        // Header row
        let header =
            Row::new(result.columns.iter().map(|c| {
                Cell::from(c.clone()).style(Style::default().add_modifier(Modifier::BOLD))
            }))
            .height(1);

        // Data rows with scroll offset
        let visible_rows = inner.height.saturating_sub(2) as usize; // Account for header and border
        let rows: Vec<Row> = result
            .rows
            .iter()
            .skip(scroll_offset)
            .take(visible_rows)
            .enumerate()
            .map(|(i, row)| {
                let style = if i % 2 == 0 {
                    Style::default()
                } else {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                };

                Row::new(row.iter().enumerate().map(|(col_idx, cell)| {
                    // Extract width from constraint for this column
                    let max_width = if let Some(Constraint::Length(w)) = widths.get(col_idx) {
                        *w as usize
                    } else {
                        50 // fallback
                    };

                    let display = truncate_cell(cell, max_width);
                    Cell::from(display)
                }))
                .style(style)
            })
            .collect();

        let table = Table::new(rows, widths).header(header);

        frame.render_widget(table, inner);

        // Show scroll indicator if there are more rows
        let total_rows = result.rows.len();
        if total_rows > visible_rows {
            let indicator = format!(
                " [{}-{}/{}] ",
                scroll_offset + 1,
                (scroll_offset + visible_rows).min(total_rows),
                total_rows
            );
            let indicator_area = Rect {
                x: area.x + area.width.saturating_sub(indicator.len() as u16 + 2),
                y: area.y + area.height - 1,
                width: indicator.len() as u16,
                height: 1,
            };
            let indicator_widget =
                Paragraph::new(indicator).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(indicator_widget, indicator_area);
        }
    }
}

/// Calculate column widths based on content, scaled to fit available space.
fn calculate_column_widths(
    headers: &[String],
    rows: &[Vec<String>],
    available_width: u16,
) -> Vec<Constraint> {
    const MIN_WIDTH: u16 = 4;
    const MAX_WIDTH: u16 = 50;
    const PADDING: u16 = 2;
    const SAMPLE_ROWS: usize = 50;

    if headers.is_empty() {
        return vec![];
    }

    let ideal_widths: Vec<u16> = headers
        .iter()
        .enumerate()
        .map(|(col_idx, header)| {
            let mut max_width = header.chars().count();

            let sample_size = rows.len().min(SAMPLE_ROWS);
            for row in rows.iter().take(sample_size) {
                if let Some(cell) = row.get(col_idx) {
                    let first_line = cell.lines().next().unwrap_or(cell);
                    let cell_width = first_line.chars().count();
                    max_width = max_width.max(cell_width);
                }
            }

            (max_width as u16 + PADDING).clamp(MIN_WIDTH, MAX_WIDTH)
        })
        .collect();

    let total_ideal: u16 = ideal_widths.iter().sum();

    if total_ideal <= available_width {
        return ideal_widths
            .into_iter()
            .map(Constraint::Length)
            .collect();
    }

    // Scale down proportionally
    let separator_space = headers.len().saturating_sub(1) as u16;
    let usable_width = available_width.saturating_sub(separator_space);

    if usable_width == 0 || total_ideal == 0 {
        return vec![Constraint::Length(MIN_WIDTH); headers.len()];
    }

    let scale_factor = usable_width as f64 / total_ideal as f64;

    ideal_widths
        .into_iter()
        .map(|w| {
            let scaled = (w as f64 * scale_factor).round() as u16;
            Constraint::Length(scaled.max(MIN_WIDTH))
        })
        .collect()
}

fn truncate_cell(s: &str, max_chars: usize) -> String {
    // Handle newlines - show first line only
    let first_line = s.lines().next().unwrap_or(s);
    let char_count = first_line.chars().count();

    if char_count <= max_chars {
        first_line.to_string()
    } else {
        let truncated: String = first_line
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod calculate_column_widths_tests {
        use super::*;

        const WIDE_SCREEN: u16 = 200; // Plenty of space

        #[test]
        fn empty_headers_returns_empty_vec() {
            let headers: Vec<String> = vec![];
            let rows: Vec<Vec<String>> = vec![];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 0);
        }

        #[test]
        fn single_column_uses_header_width_plus_padding() {
            let headers = vec!["name".to_string()];
            let rows: Vec<Vec<String>> = vec![];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 1);
            // "name" = 4 chars + 2 padding = 6, clamped to MIN_WIDTH (4) = 6
            assert_eq!(result[0], Constraint::Length(6));
        }

        #[test]
        fn uses_max_of_header_and_cell_widths() {
            let headers = vec!["id".to_string(), "name".to_string()];
            let rows = vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 2);
            // id: max(2, 1) + 2 = 4
            assert_eq!(result[0], Constraint::Length(4));
            // name: max(4, 5) + 2 = 7
            assert_eq!(result[1], Constraint::Length(7));
        }

        #[test]
        fn respects_max_width_constraint() {
            let headers = vec!["description".to_string()];
            let long_text = "a".repeat(100);
            let rows = vec![vec![long_text]];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 1);
            // Should be capped at MAX_WIDTH (50)
            assert_eq!(result[0], Constraint::Length(50));
        }

        #[test]
        fn handles_multibyte_characters_correctly() {
            let headers = vec!["名前".to_string()];
            let rows = vec![vec!["日本語テスト".to_string()]];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 1);
            // "日本語テスト" = 6 chars + 2 padding = 8
            assert_eq!(result[0], Constraint::Length(8));
        }

        #[test]
        fn only_considers_first_line_for_multiline_cells() {
            let headers = vec!["text".to_string()];
            let rows = vec![vec![
                "short\nvery long second line that should be ignored".to_string(),
            ]];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 1);
            // "short" = 5 chars, max(4, 5) + 2 = 7
            assert_eq!(result[0], Constraint::Length(7));
        }

        #[test]
        fn handles_multiple_columns_independently() {
            let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
            let rows = vec![
                vec![
                    "1".to_string(),
                    "Alice".to_string(),
                    "alice@example.com".to_string(),
                ],
                vec![
                    "22".to_string(),
                    "Bob Smith Jr.".to_string(),
                    "bob@ex.com".to_string(),
                ],
            ];

            let result = calculate_column_widths(&headers, &rows, WIDE_SCREEN);

            assert_eq!(result.len(), 3);
            // id: max(2, 2) + 2 = 4
            assert_eq!(result[0], Constraint::Length(4));
            // name: max(4, 13) + 2 = 15
            assert_eq!(result[1], Constraint::Length(15));
            // email: max(5, 17) + 2 = 19
            assert_eq!(result[2], Constraint::Length(19));
        }

        #[test]
        fn scales_down_when_total_exceeds_available_width() {
            // 10 columns each needing ~10 width = 100 ideal
            let headers: Vec<String> = (0..10).map(|i| format!("col_{:02}", i)).collect();
            let rows: Vec<Vec<String>> = vec![];

            // Only 50 available - should scale down
            let result = calculate_column_widths(&headers, &rows, 50);

            assert_eq!(result.len(), 10);
            // All columns should be scaled down proportionally
            let total: u16 = result
                .iter()
                .map(|c| match c {
                    Constraint::Length(w) => *w,
                    _ => 0,
                })
                .sum();
            // Total should fit within available space (accounting for separators)
            assert!(total <= 50, "Total {} should be <= 50", total);
        }

        #[test]
        fn maintains_min_width_even_when_scaling() {
            let headers: Vec<String> = (0..20).map(|i| format!("c{}", i)).collect();
            let rows: Vec<Vec<String>> = vec![];

            // Very narrow - each column should still have MIN_WIDTH
            let result = calculate_column_widths(&headers, &rows, 40);

            for (i, constraint) in result.iter().enumerate() {
                if let Constraint::Length(w) = constraint {
                    assert!(*w >= 4, "Column {} width {} should be >= MIN_WIDTH 4", i, w);
                }
            }
        }
    }

    #[test]
    fn short_string_returns_unchanged() {
        let result = truncate_cell("hello", 10);

        assert_eq!(result, "hello");
    }

    #[test]
    fn exact_length_returns_unchanged() {
        let result = truncate_cell("hello", 5);

        assert_eq!(result, "hello");
    }

    #[test]
    fn long_string_truncates_with_ellipsis() {
        let result = truncate_cell("hello world", 8);

        assert_eq!(result, "hello...");
    }

    #[test]
    fn multibyte_characters_count_correctly() {
        let result = truncate_cell("こんにちは世界", 5);

        assert_eq!(result, "こん...");
    }

    #[rstest]
    #[case("日本語テスト", 10, "日本語テスト")]
    #[case("日本語テスト", 5, "日本...")]
    #[case("日本語テスト", 4, "日...")]
    #[case("日本語テスト", 3, "...")]
    #[case("SELECT * FROM 日本語テーブル", 15, "SELECT * FRO...")]
    fn multibyte_truncation_is_safe(
        #[case] input: &str,
        #[case] max: usize,
        #[case] expected: &str,
    ) {
        let result = truncate_cell(input, max);

        assert_eq!(result, expected);
        assert!(result.chars().count() <= max);
    }

    #[test]
    fn newline_shows_first_line_only() {
        let result = truncate_cell("first\nsecond\nthird", 20);

        assert_eq!(result, "first");
    }

    #[test]
    fn newline_with_truncation_applies_to_first_line() {
        let result = truncate_cell("this is a long first line\nsecond", 10);

        assert_eq!(result, "this is...");
    }

    #[test]
    fn empty_string_returns_empty() {
        let result = truncate_cell("", 10);

        assert_eq!(result, "");
    }

    #[test]
    fn zero_max_chars_returns_ellipsis_only() {
        let result = truncate_cell("hello", 0);

        assert_eq!(result, "...");
    }

    #[rstest]
    #[case(1, "...")]
    #[case(2, "...")]
    #[case(3, "...")]
    #[case(4, "h...")]
    #[case(5, "he...")]
    fn small_max_chars_handles_edge_cases(#[case] max: usize, #[case] expected: &str) {
        let result = truncate_cell("hello world", max);

        assert_eq!(result, expected);
    }
}
