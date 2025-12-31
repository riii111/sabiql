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
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let is_focused = state.focused_pane == FocusedPane::Result;

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

        let result = Self::current_result(state);
        let title = Self::build_title(result, state);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let max_offset = if let Some(result) = result {
            if result.is_error() {
                Self::render_error(frame, area, result, block);
                0
            } else if result.rows.is_empty() {
                Self::render_empty(frame, area, block);
                0
            } else {
                Self::render_table(
                    frame,
                    area,
                    result,
                    block,
                    state.result_scroll_offset,
                    state.result_horizontal_offset,
                )
            }
        } else {
            Self::render_placeholder(frame, area, block);
            0
        };
        state.result_max_horizontal_offset = max_offset;
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
        horizontal_offset: usize,
    ) -> usize {
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if result.columns.is_empty() {
            return 0;
        }

        let all_ideal_widths = calculate_ideal_widths(&result.columns, &result.rows);
        let max_offset = calculate_max_offset(&all_ideal_widths, inner.width);
        let clamped_offset = horizontal_offset.min(max_offset);

        let (viewport_indices, viewport_widths) =
            select_viewport_columns(&all_ideal_widths, clamped_offset, inner.width);

        if viewport_indices.is_empty() {
            return max_offset;
        }

        let widths: Vec<Constraint> = viewport_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let col_name = result.columns.get(idx).map(|s| s.as_str()).unwrap_or("");
            Cell::from(col_name.to_string())
        }))
        .style(
            Style::default()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        )
        .height(1);

        // -3: header (1) + scroll indicators (2)
        let data_rows_visible = inner.height.saturating_sub(3) as usize;
        let scroll_viewport_size = data_rows_visible;
        let rows: Vec<Row> = result
            .rows
            .iter()
            .skip(scroll_offset)
            .take(data_rows_visible)
            .enumerate()
            .map(|(i, row)| {
                let style = if i % 2 == 0 {
                    Style::default()
                } else {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                };

                Row::new(viewport_indices.iter().zip(viewport_widths.iter()).map(
                    |(&orig_idx, &col_width)| {
                        let cell = row.get(orig_idx).map(|s| s.as_str()).unwrap_or("");
                        let display = truncate_cell(cell, col_width as usize);
                        Cell::from(display)
                    },
                ))
                .style(style)
            })
            .collect();

        let table = Table::new(rows, widths).header(header);

        frame.render_widget(table, inner);

        // Scroll indicators (pass inner area, not outer with border)
        let total_rows = result.rows.len();
        let total_cols = result.columns.len();

        use super::scroll_indicator::{
            HorizontalScrollParams, VerticalScrollParams, render_horizontal_scroll_indicator,
            render_vertical_scroll_indicator_bar,
        };
        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: scroll_viewport_size,
                total_items: total_rows,
            },
        );
        render_horizontal_scroll_indicator(
            frame,
            inner,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: viewport_indices.len(),
                total_items: total_cols,
            },
        );

        max_offset
    }
}

/// Calculate ideal widths for all columns (no scaling, just content-based).
fn calculate_ideal_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<u16> {
    const MIN_WIDTH: u16 = 4;
    const MAX_WIDTH: u16 = 50;
    const PADDING: u16 = 2;
    const SAMPLE_ROWS: usize = 50;

    headers
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
        .collect()
}

/// Select columns that fit within available width (viewport-based).
/// Returns (column_indices, column_widths) for columns that fit.
fn select_viewport_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
) -> (Vec<usize>, Vec<u16>) {
    let mut indices = Vec::new();
    let mut widths = Vec::new();
    let mut used_width: u16 = 0;

    for (i, &width) in all_widths.iter().enumerate().skip(horizontal_offset) {
        // Account for column separator (1 char) except for first column
        let separator = if indices.is_empty() { 0 } else { 1 };
        let needed = width + separator;

        if used_width + needed <= available_width {
            used_width += needed;
            indices.push(i);
            widths.push(width);
        } else {
            break;
        }
    }

    // Ensure at least one column is shown (even if it doesn't fully fit)
    if indices.is_empty() && horizontal_offset < all_widths.len() {
        indices.push(horizontal_offset);
        widths.push(all_widths[horizontal_offset].min(available_width));
    }

    (indices, widths)
}

fn calculate_max_offset(all_widths: &[u16], available_width: u16) -> usize {
    if all_widths.is_empty() {
        return 0;
    }

    let mut sum: u16 = 0;
    let mut cols_from_right = 0;

    for (i, &width) in all_widths.iter().rev().enumerate() {
        let separator = if i == 0 { 0 } else { 1 };
        let needed = width + separator;

        if sum + needed <= available_width {
            sum += needed;
            cols_from_right += 1;
        } else {
            break;
        }
    }

    let cols_from_right = cols_from_right.max(1);
    all_widths.len().saturating_sub(cols_from_right)
}

fn truncate_cell(s: &str, max_chars: usize) -> String {
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

    mod calculate_ideal_widths_tests {
        use super::*;

        #[test]
        fn empty_headers_returns_empty_vec() {
            let headers: Vec<String> = vec![];
            let rows: Vec<Vec<String>> = vec![];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 0);
        }

        #[test]
        fn single_column_uses_header_width_plus_padding() {
            let headers = vec!["name".to_string()];
            let rows: Vec<Vec<String>> = vec![];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            // "name" = 4 chars + 2 padding = 6
            assert_eq!(result[0], 6);
        }

        #[test]
        fn uses_max_of_header_and_cell_widths() {
            let headers = vec!["id".to_string(), "name".to_string()];
            let rows = vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 2);
            // id: max(2, 1) + 2 = 4
            assert_eq!(result[0], 4);
            // name: max(4, 5) + 2 = 7
            assert_eq!(result[1], 7);
        }

        #[test]
        fn respects_max_width_constraint() {
            let headers = vec!["description".to_string()];
            let long_text = "a".repeat(100);
            let rows = vec![vec![long_text]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            // Should be capped at MAX_WIDTH (50)
            assert_eq!(result[0], 50);
        }

        #[test]
        fn handles_multibyte_characters_correctly() {
            let headers = vec!["名前".to_string()];
            let rows = vec![vec!["日本語テスト".to_string()]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            // "日本語テスト" = 6 chars + 2 padding = 8
            assert_eq!(result[0], 8);
        }

        #[test]
        fn only_considers_first_line_for_multiline_cells() {
            let headers = vec!["text".to_string()];
            let rows = vec![vec![
                "short\nvery long second line that should be ignored".to_string(),
            ]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            // "short" = 5 chars, max(4, 5) + 2 = 7
            assert_eq!(result[0], 7);
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

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 3);
            // id: max(2, 2) + 2 = 4
            assert_eq!(result[0], 4);
            // name: max(4, 13) + 2 = 15
            assert_eq!(result[1], 15);
            // email: max(5, 17) + 2 = 19
            assert_eq!(result[2], 19);
        }
    }

    mod select_viewport_columns_tests {
        use super::*;

        #[test]
        fn selects_columns_that_fit() {
            let widths = vec![10, 10, 10, 10]; // 4 columns, each 10 wide
            let available = 35; // Room for 3 columns (10 + 1 + 10 + 1 + 10 = 32)

            let (indices, selected_widths) = select_viewport_columns(&widths, 0, available);

            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected_widths, vec![10, 10, 10]);
        }

        #[test]
        fn respects_horizontal_offset() {
            let widths = vec![10, 10, 10, 10];
            let available = 25; // Room for 2 columns from offset

            let (indices, selected_widths) = select_viewport_columns(&widths, 1, available);

            assert_eq!(indices, vec![1, 2]);
            assert_eq!(selected_widths, vec![10, 10]);
        }

        #[test]
        fn ensures_at_least_one_column() {
            let widths = vec![100]; // One very wide column
            let available = 20; // Not enough space

            let (indices, selected_widths) = select_viewport_columns(&widths, 0, available);

            assert_eq!(indices, vec![0]);
            assert_eq!(selected_widths, vec![20]); // Capped to available
        }

        #[test]
        fn handles_empty_widths() {
            let widths: Vec<u16> = vec![];
            let available = 100;

            let (indices, selected_widths) = select_viewport_columns(&widths, 0, available);

            assert!(indices.is_empty());
            assert!(selected_widths.is_empty());
        }

        #[test]
        fn offset_beyond_columns_returns_empty() {
            let widths = vec![10, 10];
            let available = 100;

            let (indices, selected_widths) = select_viewport_columns(&widths, 5, available);

            assert!(indices.is_empty());
            assert!(selected_widths.is_empty());
        }

        #[test]
        fn accounts_for_separators() {
            let widths = vec![10, 10, 10];
            let available = 31; // 10 + 1 + 10 + 1 + 10 = 32, so only 2 fit

            let (indices, _) = select_viewport_columns(&widths, 0, available);

            assert_eq!(indices.len(), 2);
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
