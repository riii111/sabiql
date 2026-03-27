use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table as RatatuiTable, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::viewport::{
    ColumnWidthConfig, MAX_COL_WIDTH, SelectionContext, ViewportPlan, select_viewport_columns,
};
use crate::app::ports::DdlGenerator;
use crate::app::services::AppServices;
use crate::domain::Table;
use crate::ui::primitives::atoms::panel_block;
use crate::ui::primitives::utils::text_utils::{
    MIN_COL_WIDTH, PADDING, calculate_header_min_widths,
};
use crate::ui::theme::Theme;

pub struct Inspector;

impl Inspector {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        services: &AppServices,
        now: Instant,
    ) -> ViewportPlan {
        let is_focused = state.ui.focused_pane == FocusedPane::Inspector;
        let [tab_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

        Self::render_tab_bar(frame, tab_area, state);
        Self::render_content(frame, content_area, state, is_focused, services, now)
    }

    fn render_tab_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let tabs: Vec<Span> = InspectorTab::all()
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_selected = *tab == state.ui.inspector_tab;
                let style = if is_selected {
                    Style::default()
                        .fg(Theme::TAB_ACTIVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Theme::TAB_INACTIVE)
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
        services: &AppServices,
        now: Instant,
    ) -> ViewportPlan {
        let block = panel_block(" [2] Inspector ", is_focused);

        if let Some(table) = &state.session.table_detail() {
            let inner = block.inner(area);
            frame.render_widget(block, area);

            match state.ui.inspector_tab {
                InspectorTab::Info => {
                    Self::render_info(frame, inner, table, state.ui.inspector_scroll_offset);
                    ViewportPlan::default()
                }
                InspectorTab::Columns => Self::render_columns(
                    frame,
                    inner,
                    table,
                    state.ui.inspector_scroll_offset,
                    state.ui.inspector_horizontal_offset,
                    &state.ui.inspector_viewport_plan,
                ),
                InspectorTab::Indexes => {
                    Self::render_indexes(frame, inner, table, state.ui.inspector_scroll_offset);
                    ViewportPlan::default()
                }
                InspectorTab::ForeignKeys => {
                    Self::render_foreign_keys(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::Rls => {
                    Self::render_rls(frame, inner, table, state.ui.inspector_scroll_offset);
                    ViewportPlan::default()
                }
                InspectorTab::Triggers => {
                    Self::render_triggers(frame, inner, table, state.ui.inspector_scroll_offset);
                    ViewportPlan::default()
                }
                InspectorTab::Ddl => {
                    Self::render_ddl(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset,
                        &*services.ddl_generator,
                        &state.flash_timers,
                        now,
                    );
                    ViewportPlan::default()
                }
            }
        } else {
            let content = Paragraph::new("(select a table)")
                .block(block)
                .style(Style::default().fg(Theme::PLACEHOLDER_TEXT));
            frame.render_widget(content, area);
            ViewportPlan::default()
        }
    }

    fn render_info(frame: &mut Frame, area: Rect, table: &Table, scroll_offset: usize) {
        let label_style = Style::default().add_modifier(Modifier::BOLD);
        let none_style = Style::default().fg(Theme::PLACEHOLDER_TEXT);

        let owner_value = table.owner.as_deref().unwrap_or("(none)");
        let comment_value = table.comment.as_deref().unwrap_or("(none)");
        let row_count_value = table
            .row_count_estimate
            .map_or_else(|| "(none)".to_string(), |n| format!("~{n}"));

        let owner_style = if table.owner.is_some() {
            Style::default()
        } else {
            none_style
        };
        let comment_style = if table.comment.is_some() {
            Style::default()
        } else {
            none_style
        };
        let row_count_style = if table.row_count_estimate.is_some() {
            Style::default()
        } else {
            none_style
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Owner:   ", label_style),
                Span::styled(owner_value, owner_style),
            ]),
            Line::from(vec![
                Span::styled("Comment: ", label_style),
                Span::styled(comment_value, comment_style),
            ]),
            Line::from(vec![
                Span::styled("Rows:    ", label_style),
                Span::styled(row_count_value, row_count_style),
            ]),
            Line::from(vec![
                Span::styled("Schema:  ", label_style),
                Span::raw(&table.schema),
            ]),
            Line::from(vec![
                Span::styled("Table:   ", label_style),
                Span::raw(&table.name),
            ]),
        ];

        let total_lines = lines.len();
        let visible_lines = area.height as usize;

        use crate::ui::primitives::atoms::scroll_indicator::clamp_scroll_offset;
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((clamped_scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);
    }

    fn render_columns(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        horizontal_offset: usize,
        stored_plan: &ViewportPlan,
    ) -> ViewportPlan {
        let available_width = area.width.saturating_sub(2);
        if table.columns.is_empty() {
            let msg = Paragraph::new("No columns");
            frame.render_widget(msg, area);
            return ViewportPlan::default();
        }

        let headers = vec!["Name", "Type", "Null", "PK", "Default", "Comment"];

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
                    col.comment.clone().unwrap_or_default(),
                ]
            })
            .collect();

        let header_min_widths = calculate_header_min_widths(&headers);
        let sample: &[Vec<String>] = if data_rows.len() > 50 {
            &data_rows[..50]
        } else {
            &data_rows
        };
        let (all_ideal_widths, _) = calculate_column_widths(&headers, sample);
        let current_min_widths_sum: u16 = header_min_widths.iter().sum();
        let current_ideal_widths_sum: u16 = all_ideal_widths.iter().sum();
        let current_ideal_widths_max: u16 = all_ideal_widths.iter().copied().max().unwrap_or(0);

        let plan = if stored_plan.needs_recalculation(
            all_ideal_widths.len(),
            available_width,
            current_min_widths_sum,
            current_ideal_widths_sum,
            current_ideal_widths_max,
        ) {
            ViewportPlan::calculate(&all_ideal_widths, &header_min_widths, available_width)
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
            available_width,
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

        // Header row
        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let text = headers.get(idx).copied().unwrap_or("");
            Cell::from(text)
        }))
        .style(
            Style::default()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD)
                .fg(Theme::TEXT_PRIMARY),
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
                let base_style = if (row_idx - clamped_scroll_offset) % 2 == 1 {
                    Style::default().bg(Theme::STRIPED_ROW_BG)
                } else {
                    Style::default()
                };

                Row::new(viewport_indices.iter().zip(viewport_widths.iter()).map(
                    |(&col_idx, &col_width)| {
                        let text = row.get(col_idx).map_or("", String::as_str);
                        let display = truncate_cell(text, col_width as usize);

                        // Special styling for PK and Comment columns
                        let cell_style = if col_idx == 3 && !text.is_empty() {
                            Style::default().fg(Theme::TEXT_ACCENT)
                        } else if col_idx == 5 {
                            Style::default().fg(Theme::TEXT_MUTED)
                        } else {
                            Style::default()
                        };
                        Cell::from(display).style(cell_style)
                    },
                ))
                .style(base_style)
            })
            .collect();

        let table_widget = RatatuiTable::new(rows, widths).header(header);
        frame.render_widget(table_widget, area);

        use crate::ui::primitives::atoms::scroll_indicator::{
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
                viewport_size: plan.column_count, // Use fixed count, not actual displayed (may include bonus)
                total_items: headers.len(),
            },
        );

        plan
    }

    fn render_indexes(frame: &mut Frame, area: Rect, table: &Table, scroll_offset: usize) {
        let headers = ["Name", "Columns", "Type", "Unique"];
        // Width sampling only — row_fn rebuilds cell text for actual rendering
        let data_rows: Vec<Vec<String>> = table
            .indexes
            .iter()
            .take(50)
            .map(|idx| {
                vec![
                    idx.name.clone(),
                    idx.columns.join(", "),
                    format!("{:?}", idx.index_type).to_lowercase(),
                    if idx.is_unique {
                        "✓".to_string()
                    } else {
                        String::new()
                    },
                ]
            })
            .collect();
        let (col_widths, _) = calculate_column_widths(&headers, &data_rows);
        let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

        use crate::ui::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers: &headers,
                widths: &widths,
                total_items: table.indexes.len(),
                empty_message: "No indexes",
            },
            scroll_offset,
            |idx| {
                let index = &table.indexes[idx];
                vec![
                    Cell::from(index.name.clone()),
                    Cell::from(index.columns.join(", ")),
                    Cell::from(format!("{:?}", index.index_type).to_lowercase()),
                    Cell::from(if index.is_unique { "✓" } else { "" }),
                ]
            },
        );
    }

    fn render_foreign_keys(frame: &mut Frame, area: Rect, table: &Table, scroll_offset: usize) {
        let headers = ["Name", "Columns", "References"];
        // Width sampling only — row_fn rebuilds cell text for actual rendering
        let data_rows: Vec<Vec<String>> = table
            .foreign_keys
            .iter()
            .take(50)
            .map(|fk| {
                vec![
                    fk.name.clone(),
                    fk.from_columns.join(", "),
                    format!(
                        "{}.{}({})",
                        fk.to_schema,
                        fk.to_table,
                        fk.to_columns.join(", ")
                    ),
                ]
            })
            .collect();
        let (col_widths, _) = calculate_column_widths(&headers, &data_rows);
        let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

        use crate::ui::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers: &headers,
                widths: &widths,
                total_items: table.foreign_keys.len(),
                empty_message: "No foreign keys",
            },
            scroll_offset,
            |idx| {
                let fk = &table.foreign_keys[idx];
                vec![
                    Cell::from(fk.name.clone()),
                    Cell::from(fk.from_columns.join(", ")),
                    Cell::from(format!(
                        "{}.{}({})",
                        fk.to_schema,
                        fk.to_table,
                        fk.to_columns.join(", ")
                    )),
                ]
            },
        );
    }

    fn render_rls(frame: &mut Frame, area: Rect, table: &Table, scroll_offset: usize) {
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
                            Theme::STATUS_SUCCESS
                        } else {
                            Theme::STATUS_ERROR
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
                            lines.push(Line::from(format!(
                                "    USING: {}",
                                truncate_cell(qual, 50)
                            )));
                        }
                    }
                }

                let total_lines = lines.len();
                let visible_lines = area.height as usize;

                use crate::ui::primitives::atoms::scroll_indicator::{
                    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
                };
                let clamped_scroll_offset =
                    clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

                let paragraph = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((clamped_scroll_offset as u16, 0));
                frame.render_widget(paragraph, area);

                render_vertical_scroll_indicator_bar(
                    frame,
                    area,
                    VerticalScrollParams {
                        position: clamped_scroll_offset,
                        viewport_size: visible_lines,
                        total_items: total_lines,
                    },
                );
            }
        }
    }

    fn render_triggers(frame: &mut Frame, area: Rect, table: &Table, scroll_offset: usize) {
        let headers = ["Name", "Timing", "Event", "Function", "SecDef"];
        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
        ];

        use crate::ui::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers: &headers,
                widths: &widths,
                total_items: table.triggers.len(),
                empty_message: "No triggers",
            },
            scroll_offset,
            |idx| {
                let trigger = &table.triggers[idx];
                let events_str = trigger
                    .events
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("/");
                vec![
                    Cell::from(trigger.name.clone()),
                    Cell::from(trigger.timing.to_string()),
                    Cell::from(events_str),
                    Cell::from(trigger.function_name.clone()),
                    Cell::from(if trigger.security_definer {
                        "\u{2713}"
                    } else {
                        ""
                    }),
                ]
            },
        );
    }

    fn render_ddl(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        ddl_gen: &dyn DdlGenerator,
        flash_timers: &crate::app::model::shared::flash_timer::FlashTimerStore,
        now: Instant,
    ) {
        let ddl = ddl_gen.generate_ddl(table);

        let total_lines = ddl.lines().count();
        let visible_lines = area.height as usize;

        use crate::ui::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
        };
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

        let flash_active =
            flash_timers.is_active(crate::app::model::shared::flash_timer::FlashId::Ddl, now);

        let mut lines: Vec<Line> = ddl
            .lines()
            .map(|l| Line::from(l.to_string()).style(Style::default().fg(Theme::TEXT_PRIMARY)))
            .collect();

        crate::ui::primitives::atoms::apply_yank_flash(&mut lines, flash_active);

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((clamped_scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        render_vertical_scroll_indicator_bar(
            frame,
            area,
            VerticalScrollParams {
                position: clamped_scroll_offset,
                viewport_size: visible_lines,
                total_items: total_lines,
            },
        );
    }
}

fn calculate_column_widths(headers: &[&str], rows: &[Vec<String>]) -> (Vec<u16>, u16) {
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
            true_width.clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
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
        format!("{truncated}...")
    }
}
