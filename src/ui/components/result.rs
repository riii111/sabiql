use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, Wrap};

use super::atoms::panel_block_highlight;

use super::text_utils::{MIN_COL_WIDTH, PADDING, calculate_header_min_widths};
use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;
use crate::app::ui_state::RESULT_INNER_OVERHEAD;
use crate::app::viewport::{
    ColumnWidthConfig, MAX_COL_WIDTH, SelectionContext, ViewportPlan, select_viewport_columns,
};
use crate::domain::{QueryResult, QuerySource};

pub struct ResultPane;

impl ResultPane {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) -> ViewportPlan {
        let is_focused = state.ui.focused_pane == FocusedPane::Result;
        let should_highlight = state
            .query
            .result_highlight_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);

        let result = Self::current_result(state);
        let title = Self::build_title(result, state);

        let block = panel_block_highlight(&title, is_focused, should_highlight);

        if let Some(result) = result {
            if result.is_error() {
                Self::render_error(frame, area, result, block);
                ViewportPlan::default()
            } else if result.rows.is_empty() {
                Self::render_empty(frame, area, block);
                ViewportPlan::default()
            } else {
                Self::render_table(
                    frame,
                    area,
                    result,
                    block,
                    state.ui.result_scroll_offset,
                    state.ui.result_horizontal_offset,
                    &state.ui.result_viewport_plan,
                )
            }
        } else {
            Self::render_placeholder(frame, area, block);
            ViewportPlan::default()
        }
    }

    fn current_result(state: &AppState) -> Option<&QueryResult> {
        match state.query.history_index {
            None => state.query.current_result.as_ref(),
            Some(i) => state.query.result_history.get(i),
        }
    }

    fn build_title(result: Option<&QueryResult>, state: &AppState) -> String {
        match result {
            None => " [3] Result ".to_string(),
            Some(r) => {
                let source_badge = match r.source {
                    QuerySource::Preview => "PREVIEW".to_string(),
                    QuerySource::Adhoc => {
                        if let Some(idx) = state.query.history_index {
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
        stored_plan: &ViewportPlan,
    ) -> ViewportPlan {
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if result.columns.is_empty() {
            return ViewportPlan::default();
        }

        let header_min_widths = calculate_header_min_widths(&result.columns);
        let all_ideal_widths = calculate_ideal_widths(&result.columns, &result.rows);
        let current_min_widths_sum: u16 = header_min_widths.iter().sum();
        let current_ideal_widths_sum: u16 = all_ideal_widths.iter().sum();
        let current_ideal_widths_max: u16 = all_ideal_widths.iter().copied().max().unwrap_or(0);

        let plan = if stored_plan.needs_recalculation(
            all_ideal_widths.len(),
            inner.width,
            current_min_widths_sum,
            current_ideal_widths_sum,
            current_ideal_widths_max,
        ) {
            ViewportPlan::calculate(&all_ideal_widths, &header_min_widths, inner.width)
        } else {
            stored_plan.clone()
        };

        let clamped_offset = horizontal_offset.min(plan.max_offset);

        let config = ColumnWidthConfig {
            ideal_widths: &all_ideal_widths,
            min_widths: &header_min_widths,
        };
        let ctx = SelectionContext {
            horizontal_offset: clamped_offset,
            available_width: inner.width,
            fixed_count: Some(plan.column_count),
            max_offset: plan.max_offset,
            slack_policy: plan.slack_policy,
        };
        let (viewport_indices, viewport_widths) = select_viewport_columns(&config, &ctx);

        if viewport_indices.is_empty() {
            return plan;
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

        let data_rows_visible = inner.height.saturating_sub(RESULT_INNER_OVERHEAD) as usize;
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
                viewport_size: plan.column_count, // Use fixed count, not actual displayed (may include bonus)
                total_items: total_cols,
            },
        );

        plan
    }
}

/// Calculate ideal widths for all columns (no scaling, just content-based).
fn calculate_ideal_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<u16> {
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

            (max_width as u16 + PADDING).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
        })
        .collect()
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
            // Should be capped at MAX_COL_WIDTH (50)
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
