use std::time::Instant;

use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::domain::{QueryResult, QuerySource};

pub struct ResultPane;

impl ResultPane {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        // Check if we should show highlight (flash effect on new results)
        let should_highlight = state
            .result_highlight_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);

        let border_style = if should_highlight {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
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
            None => "Result".to_string(),
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
                    format!("Result [{}] ERROR", source_badge)
                } else {
                    format!(
                        "Result [{}] ({}, {}ms)",
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

        // Calculate column widths based on content
        let col_count = result.columns.len();
        let widths: Vec<Constraint> = if col_count <= 5 {
            vec![Constraint::Percentage((100 / col_count as u16).max(10)); col_count]
        } else {
            // For many columns, use min width
            vec![Constraint::Min(15); col_count]
        };

        // Header row
        let header = Row::new(
            result
                .columns
                .iter()
                .map(|c| Cell::from(c.clone()).style(Style::default().add_modifier(Modifier::BOLD))),
        )
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

                Row::new(row.iter().map(|cell| {
                    let display = truncate_cell(cell, 30);
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
            let indicator_widget = Paragraph::new(indicator)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(indicator_widget, indicator_area);
        }
    }
}

fn truncate_cell(s: &str, max_len: usize) -> String {
    // Handle newlines - show first line only
    let first_line = s.lines().next().unwrap_or(s);

    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}
