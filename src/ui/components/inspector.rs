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
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let is_focused = state.focused_pane == FocusedPane::Inspector;
        // Split into tab bar and content
        let [tab_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

        Self::render_tab_bar(frame, tab_area, state);
        Self::render_content(frame, content_area, state, is_focused);
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

    fn render_content(frame: &mut Frame, area: Rect, state: &AppState, is_focused: bool) {
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        let block = Block::default()
            .title("Inspector")
            .borders(Borders::ALL)
            .border_style(border_style);

        if let Some(table) = &state.table_detail {
            let inner = block.inner(area);
            frame.render_widget(block, area);

            match state.inspector_tab {
                InspectorTab::Columns => Self::render_columns(frame, inner, table),
                InspectorTab::Indexes => Self::render_indexes(frame, inner, table),
                InspectorTab::ForeignKeys => Self::render_foreign_keys(frame, inner, table),
                InspectorTab::Rls => Self::render_rls(frame, inner, table),
                InspectorTab::Ddl => Self::render_ddl(frame, inner, table),
            }
        } else {
            let content = Paragraph::new("(select a table)")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
        }
    }

    fn render_columns(frame: &mut Frame, area: Rect, table: &TableDetail) {
        if table.columns.is_empty() {
            let msg = Paragraph::new("No columns");
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("Name").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Null").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("PK").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Default").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .height(1);

        let rows: Vec<Row> = table
            .columns
            .iter()
            .map(|col| {
                let pk_marker = if col.is_primary_key { "●" } else { "" };
                let null_marker = if col.nullable { "✓" } else { "" };
                let default = col
                    .default
                    .as_ref()
                    .map(|d| truncate_str(d, 20))
                    .unwrap_or_default();

                Row::new(vec![
                    Cell::from(col.name.clone()),
                    Cell::from(col.data_type.clone()),
                    Cell::from(null_marker),
                    Cell::from(pk_marker).style(Style::default().fg(Color::Yellow)),
                    Cell::from(default).style(Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(30),
        ];

        let table_widget = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::default().bg(Color::DarkGray));

        frame.render_widget(table_widget, area);
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

        let rows: Vec<Row> = table
            .indexes
            .iter()
            .map(|idx| {
                let unique_marker = if idx.is_unique { "✓" } else { "" };
                let type_str = format!("{:?}", idx.index_type).to_lowercase();
                Row::new(vec![
                    Cell::from(idx.name.clone()),
                    Cell::from(idx.columns.join(", ")),
                    Cell::from(type_str),
                    Cell::from(unique_marker),
                ])
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

        let rows: Vec<Row> = table
            .foreign_keys
            .iter()
            .map(|fk| {
                let refs = format!(
                    "{}.{}({})",
                    fk.to_schema,
                    fk.to_table,
                    fk.to_columns.join(", ")
                );
                Row::new(vec![
                    Cell::from(fk.name.clone()),
                    Cell::from(fk.from_columns.join(", ")),
                    Cell::from(refs),
                ])
            })
            .collect();

        let widths = [
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(40),
        ];

        let table_widget = Table::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);
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

fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
