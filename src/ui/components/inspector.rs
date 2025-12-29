use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

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
        let max_offset = Self::render_content(frame, content_area, state, is_focused);
        state.inspector_max_horizontal_offset = max_offset;
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

    fn render_content(frame: &mut Frame, area: Rect, state: &AppState, is_focused: bool) -> usize {
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
                    state.inspector_selected_row,
                ),
                InspectorTab::Indexes => {
                    Self::render_indexes(frame, inner, table);
                    0
                }
                InspectorTab::ForeignKeys => {
                    Self::render_foreign_keys(frame, inner, table);
                    0
                }
                InspectorTab::Rls => {
                    Self::render_rls(frame, inner, table);
                    0
                }
                InspectorTab::Ddl => {
                    Self::render_ddl(frame, inner, table);
                    0
                }
            }
        } else {
            let content = Paragraph::new("(select a table)")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
            0
        }
    }

    fn render_columns(
        frame: &mut Frame,
        area: Rect,
        table: &TableDetail,
        scroll_offset: usize,
        horizontal_offset: usize,
        selected_row: usize,
    ) -> usize {
        if table.columns.is_empty() {
            let msg = Paragraph::new("No columns");
            frame.render_widget(msg, area);
            return 0;
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
                    if col.nullable { "✓".to_string() } else { String::new() },
                    if col.is_primary_key { "●".to_string() } else { String::new() },
                    col.default.clone().unwrap_or_default(),
                ]
            })
            .collect();

        let (all_ideal_widths, _) = calculate_column_widths(&headers, &data_rows);
        let max_offset = calculate_max_offset(&all_ideal_widths, area.width.saturating_sub(2));
        let clamped_offset = horizontal_offset.min(max_offset);

        let (viewport_indices, viewport_widths) =
            select_viewport_columns(&all_ideal_widths, clamped_offset, area.width.saturating_sub(2));

        if viewport_indices.is_empty() {
            return max_offset;
        }

        let widths: Vec<Constraint> = viewport_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        // Header row
        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let text = headers.get(idx).copied().unwrap_or("");
            Cell::from(text).style(Style::default().add_modifier(Modifier::BOLD))
        }))
        .height(1);

        // Visible data rows with scroll offset
        let visible_rows = area.height.saturating_sub(2) as usize; // -1 header, -1 indicator
        let total_rows = data_rows.len();

        let rows: Vec<Row> = data_rows
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_rows)
            .map(|(row_idx, row)| {
                let is_selected = row_idx == selected_row;
                let is_striped = (row_idx - scroll_offset) % 2 == 1;

                let base_style = if is_selected {
                    Style::default().bg(Color::Rgb(0x3a, 0x3a, 0x4e))
                } else if is_striped {
                    Style::default().bg(Color::Rgb(0x2a, 0x2a, 0x2e))
                } else {
                    Style::default()
                };

                Row::new(
                    viewport_indices
                        .iter()
                        .zip(viewport_widths.iter())
                        .map(|(&col_idx, &col_width)| {
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
                        }),
                )
                .style(base_style)
            })
            .collect();

        let table_widget = Table::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);

        use super::scroll_indicator::{
            render_horizontal_scroll_indicator, render_vertical_scroll_indicator,
            HorizontalScrollParams,
        };
        render_vertical_scroll_indicator(frame, area, scroll_offset, visible_rows, total_rows);
        render_horizontal_scroll_indicator(
            frame,
            area,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: viewport_indices.len(),
                total_items: headers.len(),
            },
        );

        max_offset
    }

    fn render_indexes(frame: &mut Frame, area: Rect, table: &TableDetail) {
        if table.indexes.is_empty() {
            let msg = Paragraph::new("No indexes");
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("Name").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Columns").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Unique").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .height(1);

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
        use super::scroll_indicator::render_vertical_scroll_indicator;
        render_vertical_scroll_indicator(frame, area, 0, visible_rows, total_rows);
    }

    fn render_foreign_keys(frame: &mut Frame, area: Rect, table: &TableDetail) {
        if table.foreign_keys.is_empty() {
            let msg = Paragraph::new("No foreign keys");
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("Name").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Columns").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("References").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .height(1);

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
        use super::scroll_indicator::render_vertical_scroll_indicator;
        render_vertical_scroll_indicator(frame, area, 0, visible_rows, total_rows);
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

/// Returns (clamped_widths, true_total_width)
/// - clamped_widths: widths clamped to MIN/MAX for rendering
/// - true_total_width: sum of unclamped widths (for scroll detection)
fn calculate_column_widths(headers: &[&str], rows: &[Vec<String>]) -> (Vec<u16>, u16) {
    const MIN_WIDTH: u16 = 4;
    const MAX_WIDTH: u16 = 40;
    const PADDING: u16 = 2;

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
            true_width.clamp(MIN_WIDTH, MAX_WIDTH)
        })
        .collect();

    (clamped, true_total)
}

fn select_viewport_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
) -> (Vec<usize>, Vec<u16>) {
    let mut indices = Vec::new();
    let mut widths = Vec::new();
    let mut used_width: u16 = 0;

    for (i, &width) in all_widths.iter().enumerate().skip(horizontal_offset) {
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
