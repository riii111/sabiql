use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use super::viewport_columns::{
    ColumnWidthConfig, SelectionContext, calculate_max_offset, calculate_viewport_column_count,
    select_viewport_columns,
};
use crate::app::focused_pane::FocusedPane;
use crate::app::inspector_tab::InspectorTab;
use crate::app::state::AppState;
use crate::domain::Table as TableDetail;
use crate::infra::utils::quote_ident;

pub struct Inspector;

impl Inspector {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let is_focused = state.focused_pane == FocusedPane::Inspector;
        // Split into tab bar and content
        let [tab_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

        Self::render_tab_bar(frame, tab_area, state);
        let (max_offset, column_widths, available_width, viewport_column_count, min_widths_sum) =
            Self::render_content(frame, content_area, state, is_focused);
        state.inspector_max_horizontal_offset = max_offset;
        state.inspector_column_widths = column_widths;
        state.inspector_available_width = available_width;
        state.inspector_viewport_column_count = viewport_column_count;
        state.inspector_min_widths_sum = min_widths_sum;
    }

    fn render_tab_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let tabs: Vec<Span> = InspectorTab::all()
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_selected = *tab == state.inspector_tab;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let mut spans = vec![];
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(format!("[{}]", tab.display_name()), style));
                spans
            })
            .collect();

        let line = Line::from(tabs);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }

    fn render_content(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        is_focused: bool,
    ) -> (usize, Vec<u16>, u16, usize, u16) {
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" [2] Inspector ")
            .borders(Borders::ALL)
            .border_style(border_style);

        if let Some(table) = &state.table_detail {
            let inner = block.inner(area);
            frame.render_widget(block, area);

            match state.inspector_tab {
                InspectorTab::Columns => Self::render_columns(
                    frame,
                    inner,
                    table,
                    state.inspector_scroll_offset,
                    state.inspector_horizontal_offset,
                    state.inspector_viewport_column_count,
                    state.inspector_available_width,
                    state.inspector_column_widths.len(),
                    state.inspector_min_widths_sum,
                ),
                InspectorTab::Indexes => {
                    Self::render_indexes(frame, inner, table);
                    (0, Vec::new(), 0, 0, 0)
                }
                InspectorTab::ForeignKeys => {
                    Self::render_foreign_keys(frame, inner, table);
                    (0, Vec::new(), 0, 0, 0)
                }
                InspectorTab::Rls => {
                    Self::render_rls(frame, inner, table);
                    (0, Vec::new(), 0, 0, 0)
                }
                InspectorTab::Ddl => {
                    Self::render_ddl(frame, inner, table);
                    (0, Vec::new(), 0, 0, 0)
                }
            }
        } else {
            let content = Paragraph::new("(select a table)")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
            (0, Vec::new(), 0, 0, 0)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_columns(
        frame: &mut Frame,
        area: Rect,
        table: &TableDetail,
        scroll_offset: usize,
        horizontal_offset: usize,
        stored_column_count: usize,
        stored_available_width: u16,
        stored_column_widths_len: usize,
        stored_min_widths_sum: u16,
    ) -> (usize, Vec<u16>, u16, usize, u16) {
        let available_width = area.width.saturating_sub(2);
        if table.columns.is_empty() {
            let msg = Paragraph::new("No columns");
            frame.render_widget(msg, area);
            return (0, Vec::new(), available_width, 0, 0);
        }

        let headers = vec!["Name", "Type", "Null", "PK", "Default"];

        // Build data rows
        let data_rows: Vec<Vec<String>> = table
            .columns
            .iter()
            .map(|col| {
                vec![
                    col.name.clone(),
                    col.data_type.clone(),
                    if col.nullable {
                        "✓".to_string()
                    } else {
                        String::new()
                    },
                    if col.is_primary_key {
                        "✓".to_string()
                    } else {
                        String::new()
                    },
                    col.default.clone().unwrap_or_default(),
                ]
            })
            .collect();

        let header_min_widths = calculate_header_min_widths(&headers);
        let (all_ideal_widths, _) = calculate_column_widths(&headers, &data_rows);
        let current_min_widths_sum: u16 = header_min_widths.iter().sum();

        let needs_recalc = stored_column_count == 0
            || stored_available_width != available_width
            || stored_column_widths_len != all_ideal_widths.len()
            || stored_min_widths_sum != current_min_widths_sum;

        let viewport_column_count = if needs_recalc {
            calculate_viewport_column_count(&all_ideal_widths, &header_min_widths, available_width)
        } else {
            stored_column_count
        };

        let max_offset = calculate_max_offset(all_ideal_widths.len(), viewport_column_count);
        let clamped_offset = horizontal_offset.min(max_offset);

        let config = ColumnWidthConfig {
            ideal_widths: &all_ideal_widths,
            min_widths: &header_min_widths,
        };
        let ctx = SelectionContext {
            horizontal_offset: clamped_offset,
            available_width,
            fixed_count: Some(viewport_column_count),
            max_offset,
        };
        let (viewport_indices, viewport_widths) = select_viewport_columns(&config, &ctx);

        if viewport_indices.is_empty() {
            return (
                max_offset,
                all_ideal_widths,
                available_width,
                viewport_column_count,
                current_min_widths_sum,
            );
        }

        let widths: Vec<Constraint> = viewport_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        // Header row
        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let text = headers.get(idx).copied().unwrap_or("");
            Cell::from(text)
        }))
        .style(
            Style::default()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        )
        .height(1);

        // -2: Table header (1) + scroll indicator row at bottom (1)
        // Note: area is already inner (excluding border and tab bar)
        let data_rows_visible = area.height.saturating_sub(2) as usize;
        let scroll_viewport_size = data_rows_visible;
        let total_rows = data_rows.len();

        let max_scroll_offset = total_rows.saturating_sub(data_rows_visible);
        let clamped_scroll_offset = scroll_offset.min(max_scroll_offset);

        let rows: Vec<Row> = data_rows
            .iter()
            .enumerate()
            .skip(clamped_scroll_offset)
            .take(data_rows_visible)
            .map(|(row_idx, row)| {
                let is_striped = (row_idx - clamped_scroll_offset) % 2 == 1;

                let base_style = if is_striped {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                } else {
                    Style::default()
                };

                Row::new(viewport_indices.iter().zip(viewport_widths.iter()).map(
                    |(&col_idx, &col_width)| {
                        let text = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                        let display = truncate_cell(text, col_width as usize);

                        // Special styling for PK and Default columns
                        let cell_style = if col_idx == 3 && !text.is_empty() {
                            Style::default().fg(Color::Yellow)
                        } else if col_idx == 4 {
                            Style::default().fg(Color::Gray)
                        } else {
                            Style::default()
                        };
                        Cell::from(display).style(cell_style)
                    },
                ))
                .style(base_style)
            })
            .collect();

        let table_widget = Table::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);

        use super::scroll_indicator::{
            HorizontalScrollParams, VerticalScrollParams, render_horizontal_scroll_indicator,
            render_vertical_scroll_indicator_bar,
        };
        render_vertical_scroll_indicator_bar(
            frame,
            area,
            VerticalScrollParams {
                position: clamped_scroll_offset,
                viewport_size: scroll_viewport_size,
                total_items: total_rows,
            },
        );
        render_horizontal_scroll_indicator(
            frame,
            area,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: viewport_indices.len(),
                total_items: headers.len(),
            },
        );

        (
            max_offset,
            all_ideal_widths,
            available_width,
            viewport_column_count,
            current_min_widths_sum,
        )
    }

    fn render_indexes(frame: &mut Frame, area: Rect, table: &TableDetail) {
        if table.indexes.is_empty() {
            let msg = Paragraph::new("No indexes");
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("Name"),
            Cell::from("Columns"),
            Cell::from("Type"),
            Cell::from("Unique"),
        ])
        .style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED)
                .fg(Color::White),
        )
        .height(1);

        // -2: header (1) + scroll indicator (1)
        let visible_rows = area.height.saturating_sub(2) as usize;
        let total_rows = table.indexes.len();

        let rows: Vec<Row> = table
            .indexes
            .iter()
            .enumerate()
            .take(visible_rows)
            .map(|(i, idx)| {
                let unique_marker = if idx.is_unique { "✓" } else { "" };
                let type_str = format!("{:?}", idx.index_type).to_lowercase();
                let style = if i % 2 == 1 {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(idx.name.clone()),
                    Cell::from(idx.columns.join(", ")),
                    Cell::from(type_str),
                    Cell::from(unique_marker),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
        ];

        let table_widget = Table::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);

        // Vertical scroll indicator
        use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};
        render_vertical_scroll_indicator_bar(
            frame,
            area,
            VerticalScrollParams {
                position: 0,
                viewport_size: visible_rows,
                total_items: total_rows,
            },
        );
    }

    fn render_foreign_keys(frame: &mut Frame, area: Rect, table: &TableDetail) {
        if table.foreign_keys.is_empty() {
            let msg = Paragraph::new("No foreign keys");
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("Name"),
            Cell::from("Columns"),
            Cell::from("References"),
        ])
        .style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED)
                .fg(Color::White),
        )
        .height(1);

        // -2: header (1) + scroll indicator (1)
        let visible_rows = area.height.saturating_sub(2) as usize;
        let total_rows = table.foreign_keys.len();

        let rows: Vec<Row> = table
            .foreign_keys
            .iter()
            .enumerate()
            .take(visible_rows)
            .map(|(i, fk)| {
                let refs = format!(
                    "{}.{}({})",
                    fk.to_schema,
                    fk.to_table,
                    fk.to_columns.join(", ")
                );
                let style = if i % 2 == 1 {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(fk.name.clone()),
                    Cell::from(fk.from_columns.join(", ")),
                    Cell::from(refs),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(40),
        ];

        let table_widget = Table::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);

        // Vertical scroll indicator
        use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};
        render_vertical_scroll_indicator_bar(
            frame,
            area,
            VerticalScrollParams {
                position: 0,
                viewport_size: visible_rows,
                total_items: total_rows,
            },
        );
    }

    fn render_rls(frame: &mut Frame, area: Rect, table: &TableDetail) {
        match &table.rls {
            None => {
                let msg = Paragraph::new("RLS not enabled");
                frame.render_widget(msg, area);
            }
            Some(rls) => {
                let status = if rls.enabled {
                    if rls.force {
                        "Enabled (FORCE)"
                    } else {
                        "Enabled"
                    }
                } else {
                    "Disabled"
                };

                let mut lines = vec![Line::from(vec![
                    Span::raw("Status: "),
                    Span::styled(
                        status,
                        Style::default().fg(if rls.enabled {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ])];

                if !rls.policies.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "Policies:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));

                    for policy in &rls.policies {
                        let cmd = format!("{:?}", policy.cmd).to_uppercase();
                        lines.push(Line::from(format!(
                            "  {} ({}) - {}",
                            policy.name,
                            cmd,
                            if policy.permissive {
                                "PERMISSIVE"
                            } else {
                                "RESTRICTIVE"
                            }
                        )));
                        if let Some(qual) = &policy.qual {
                            lines
                                .push(Line::from(format!("    USING: {}", truncate_str(qual, 50))));
                        }
                    }
                }

                let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
                frame.render_widget(paragraph, area);
            }
        }
    }

    fn render_ddl(frame: &mut Frame, area: Rect, table: &TableDetail) {
        // Generate a simplified DDL representation with proper identifier quoting
        let mut ddl = format!(
            "CREATE TABLE {}.{} (\n",
            quote_ident(&table.schema),
            quote_ident(&table.name)
        );

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();

            ddl.push_str(&format!(
                "  {} {}{}{}",
                quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            ));

            if i < table.columns.len() - 1 {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        // Add primary key constraint
        if let Some(pk) = &table.primary_key {
            let quoted_cols: Vec<String> = pk.iter().map(|c| quote_ident(c)).collect();
            ddl.push_str(&format!("  PRIMARY KEY ({})\n", quoted_cols.join(", ")));
        }

        ddl.push_str(");");

        let paragraph = Paragraph::new(ddl)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    }
}

const MIN_COL_WIDTH: u16 = 4;
const PADDING: u16 = 2;

fn calculate_header_min_widths(headers: &[&str]) -> Vec<u16> {
    headers
        .iter()
        .map(|h| (h.chars().count() as u16 + PADDING).max(MIN_COL_WIDTH))
        .collect()
}

/// Returns (clamped_widths, true_total_width)
/// - clamped_widths: widths clamped to MIN/MAX for rendering
/// - true_total_width: sum of unclamped widths (for scroll detection)
fn calculate_column_widths(headers: &[&str], rows: &[Vec<String>]) -> (Vec<u16>, u16) {
    const MAX_WIDTH: u16 = 40;

    let mut true_total: u16 = 0;
    let clamped: Vec<u16> = headers
        .iter()
        .enumerate()
        .map(|(col_idx, header)| {
            let mut max_width = header.chars().count();

            for row in rows.iter().take(50) {
                if let Some(cell) = row.get(col_idx) {
                    max_width = max_width.max(cell.chars().count());
                }
            }

            let true_width = max_width as u16 + PADDING;
            true_total += true_width;
            true_width.clamp(MIN_COL_WIDTH, MAX_WIDTH)
        })
        .collect();

    (clamped, true_total)
}

fn truncate_cell(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scroll_offset_clamping_with_large_offset_and_small_table() {
        let total_rows: usize = 5;
        let visible_rows: usize = 10;

        let max_scroll_offset = total_rows.saturating_sub(visible_rows);
        let clamped = 100_usize.min(max_scroll_offset);

        assert_eq!(clamped, 0);
    }

    #[test]
    fn scroll_offset_clamping_with_exact_fit() {
        let total_rows: usize = 10;
        let visible_rows: usize = 10;

        let max_scroll_offset = total_rows.saturating_sub(visible_rows);
        let clamped = 5_usize.min(max_scroll_offset);

        assert_eq!(clamped, 0);
    }

    #[test]
    fn scroll_offset_clamping_with_normal_scroll() {
        let total_rows: usize = 100;
        let visible_rows: usize = 10;

        let max_scroll_offset = total_rows.saturating_sub(visible_rows);
        let clamped = 50_usize.min(max_scroll_offset);

        assert_eq!(clamped, 50);
    }

    #[test]
    fn scroll_offset_clamping_when_offset_exceeds_max() {
        let total_rows: usize = 20;
        let visible_rows: usize = 10;

        let max_scroll_offset = total_rows.saturating_sub(visible_rows);
        let clamped = 100_usize.min(max_scroll_offset);

        assert_eq!(clamped, 10);
    }
}
