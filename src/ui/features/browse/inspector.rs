use std::borrow::Cow;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table as RatatuiTable, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::db_capabilities::{DbCapabilities, InspectorInfoField};
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::viewport::{
    ColumnWidthConfig, MAX_COL_WIDTH, SelectionContext, ViewportPlan, select_viewport_columns,
    widths_fingerprint,
};
use crate::app::present::table_storage::{inspector_flags_label, inspector_kind_label};
use crate::app::services::AppServices;
use crate::domain::{ForeignKey, Index, IndexType, Table};
use crate::primitives::atoms::panel_block;
use crate::primitives::utils::text_utils::{
    MIN_COL_WIDTH, PADDING, calculate_header_min_widths, truncate_to_width,
};
use crate::theme::ThemePalette;

pub struct Inspector;

impl Inspector {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let is_focused = state.ui.focused_pane() == FocusedPane::Inspector;
        let [tab_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        let active_tab = state
            .session
            .active_db_capabilities()
            .normalize_inspector_tab(state.ui.inspector_tab());

        Self::render_tab_bar(frame, tab_area, active_tab, state, theme);
        Self::render_content(
            frame,
            content_area,
            state,
            active_tab,
            is_focused,
            services,
            now,
            theme,
        )
    }

    fn render_tab_bar(
        frame: &mut Frame,
        area: Rect,
        active_tab: InspectorTab,
        state: &AppState,
        theme: &ThemePalette,
    ) {
        let tabs: Vec<Span> = state
            .session
            .active_db_capabilities()
            .supported_inspector_tabs()
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let is_selected = *tab == active_tab;
                let style = if is_selected {
                    Style::default()
                        .fg(theme.component.navigation.tab_active)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.component.navigation.tab_inactive)
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
        active_tab: InspectorTab,
        is_focused: bool,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let block = panel_block(" [2] Inspector ", is_focused, theme);

        if let Some(table) = &state.session.table_detail() {
            let inner = block.inner(area);
            frame.render_widget(block, area);

            match active_tab {
                InspectorTab::Info => {
                    Self::render_info(
                        frame,
                        inner,
                        state.session.active_db_capabilities(),
                        table,
                        state.ui.inspector_scroll_offset(),
                        theme,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::Columns => Self::render_columns(
                    frame,
                    inner,
                    table,
                    state.ui.inspector_scroll_offset(),
                    state.ui.inspector_horizontal_offset(),
                    state.ui.inspector_viewport_plan(),
                    theme,
                ),
                InspectorTab::Indexes => {
                    Self::render_indexes(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset(),
                        theme,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::ForeignKeys => {
                    Self::render_foreign_keys(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset(),
                        theme,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::Rls => {
                    Self::render_rls(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset(),
                        theme,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::Triggers => {
                    Self::render_triggers(
                        frame,
                        inner,
                        table,
                        state.ui.inspector_scroll_offset(),
                        theme,
                    );
                    ViewportPlan::default()
                }
                InspectorTab::Ddl => {
                    let database_type = state.session.active_database_type_or_default();
                    let ddl = services.ddl_generator.generate_ddl(database_type, table);
                    Self::render_ddl(
                        frame,
                        inner,
                        ddl,
                        state.ui.inspector_scroll_offset(),
                        &state.flash_timers,
                        now,
                        theme,
                    );
                    ViewportPlan::default()
                }
            }
        } else {
            let content = Paragraph::new("(select a table)")
                .block(block)
                .style(Style::default().fg(theme.semantic.text.placeholder));
            frame.render_widget(content, area);
            ViewportPlan::default()
        }
    }

    fn render_info(
        frame: &mut Frame,
        area: Rect,
        capabilities: &DbCapabilities,
        table: &Table,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let lines: Vec<Line> = capabilities
            .supported_inspector_info_fields()
            .iter()
            .copied()
            .map(|field| Self::render_info_field(field, table, theme))
            .collect();

        let total_lines = lines.len();
        let visible_lines = area.height as usize;

        use crate::primitives::atoms::scroll_indicator::clamp_scroll_offset;
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.semantic.text.primary))
            .wrap(Wrap { trim: false })
            .scroll((clamped_scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);
    }

    fn render_info_field<'a>(
        field: InspectorInfoField,
        table: &'a Table,
        theme: &ThemePalette,
    ) -> Line<'a> {
        match field {
            InspectorInfoField::Owner => Self::optional_info_line(
                "Owner:   ",
                table.owner.as_deref().map(Cow::Borrowed),
                theme,
            ),
            InspectorInfoField::Comment => Self::optional_info_line(
                "Comment: ",
                table.comment.as_deref().map(Cow::Borrowed),
                theme,
            ),
            InspectorInfoField::RowCount => {
                let value = table
                    .row_count_estimate
                    .map(|n| Cow::Owned(format!("~{n}")));
                Self::optional_info_line("Rows:    ", value, theme)
            }
            InspectorInfoField::Schema => Line::from(vec![
                Self::info_label("Schema:  "),
                Span::raw(&table.schema),
            ]),
            InspectorInfoField::TableName => {
                Line::from(vec![Self::info_label("Table:   "), Span::raw(&table.name)])
            }
            InspectorInfoField::TableKind => Line::from(vec![
                Self::info_label("Kind:    "),
                Span::raw(inspector_kind_label(&table.storage)),
            ]),
            InspectorInfoField::TableFlags => {
                let flags = inspector_flags_label(&table.storage);
                let value = flags
                    .as_ref()
                    .map_or(Cow::Borrowed("(none)"), |label| Cow::Owned(label.clone()));
                let style = if flags.is_some() {
                    Style::default()
                } else {
                    Style::default().fg(theme.semantic.text.placeholder)
                };
                Line::from(vec![
                    Self::info_label("Flags:   "),
                    Span::styled(value, style),
                ])
            }
        }
    }

    fn optional_info_line<'a>(
        label: &'static str,
        value: Option<Cow<'a, str>>,
        theme: &ThemePalette,
    ) -> Line<'a> {
        let none_style = Style::default().fg(theme.semantic.text.placeholder);
        let (value, style) = match value {
            Some(value) => (value, Style::default()),
            None => (Cow::Borrowed("(none)"), none_style),
        };

        Line::from(vec![Self::info_label(label), Span::styled(value, style)])
    }

    fn info_label(label: &'static str) -> Span<'static> {
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD))
    }

    fn render_columns(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        horizontal_offset: usize,
        stored_plan: &ViewportPlan,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let available_width = area.width.saturating_sub(2);
        if table.columns.is_empty() {
            let msg = Paragraph::new("No columns");
            frame.render_widget(msg, area);
            return ViewportPlan::default();
        }

        let show_read_only = table
            .columns
            .iter()
            .any(|col| col.read_only_reason().is_some());
        let mut headers = vec!["Name", "Type", "Null", "PK"];
        if show_read_only {
            headers.push("Read-only");
        }
        headers.extend(["Default", "Comment"]);

        let data_rows: Vec<Vec<String>> = table
            .columns
            .iter()
            .map(|col| {
                let mut row = vec![
                    col.name.clone(),
                    col.data_type.clone(),
                    if col.is_nullable() {
                        "✓".to_string()
                    } else {
                        String::new()
                    },
                    if col.is_primary_key() {
                        "✓".to_string()
                    } else {
                        String::new()
                    },
                ];
                if show_read_only {
                    row.push(col.read_only_reason().unwrap_or_default().to_string());
                }
                row.push(col.default.clone().unwrap_or_default());
                row.push(col.comment.clone().unwrap_or_default());
                row
            })
            .collect();

        let header_min_widths = calculate_header_min_widths(&headers);
        let sample: &[Vec<String>] = if data_rows.len() > 50 {
            &data_rows[..50]
        } else {
            &data_rows
        };
        let all_ideal_widths = calculate_column_widths(&headers, sample);
        let fingerprint = widths_fingerprint(&all_ideal_widths, &header_min_widths);
        let plan = if stored_plan.needs_recalculation(available_width, fingerprint) {
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
                .fg(theme.semantic.text.primary),
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
                    Style::default().bg(theme.component.table.striped_row_bg)
                } else {
                    Style::default()
                };

                Row::new(viewport_indices.iter().zip(viewport_widths.iter()).map(
                    |(&col_idx, &col_width)| {
                        let text = row.get(col_idx).map_or("", String::as_str);
                        let display = truncate_to_width(text, col_width as usize);

                        let read_only_col_idx = show_read_only.then_some(4);
                        let comment_col_idx = if show_read_only { 6 } else { 5 };
                        let cell_style = if col_idx == 3 && !text.is_empty() {
                            Style::default().fg(theme.semantic.text.accent)
                        } else if read_only_col_idx == Some(col_idx) && !text.is_empty() {
                            Style::default().fg(theme.semantic.status.warning)
                        } else if col_idx == comment_col_idx {
                            Style::default().fg(theme.semantic.text.muted)
                        } else {
                            Style::default()
                        };
                        Cell::from(display).style(cell_style)
                    },
                ))
                .style(base_style)
            })
            .collect();

        let table_widget = RatatuiTable::new(rows, widths)
            .header(header)
            .style(Style::default().fg(theme.semantic.text.primary));
        frame.render_widget(table_widget, area);

        use crate::primitives::atoms::scroll_indicator::{
            HorizontalScrollParams, VerticalScrollParams, render_horizontal_scroll_indicator,
            render_vertical_scroll_indicator_bar,
        };
        let has_h_scroll = plan.has_horizontal_scroll();
        render_vertical_scroll_indicator_bar(
            frame,
            area,
            VerticalScrollParams {
                position: clamped_scroll_offset,
                viewport_size: scroll_viewport_size,
                total_items: total_rows,
                has_horizontal_scrollbar: has_h_scroll,
            },
            theme,
        );
        render_horizontal_scroll_indicator(
            frame,
            area,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: plan.indicator_viewport_size(),
                total_items: headers.len(),
            },
            theme,
        );

        plan
    }

    fn render_indexes(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let show_type = table
            .indexes
            .iter()
            .any(|index| index.index_type != IndexType::Unknown);
        let has_details = table.indexes.iter().any(Index::has_index_detail);
        let headers_with_type_and_details =
            ["Name", "Columns", "Type", "Unique", "Partial", "Detail"];
        let headers_with_type = ["Name", "Columns", "Type", "Unique"];
        let headers_without_type_and_details = ["Name", "Columns", "Unique", "Partial", "Detail"];
        let headers_without_type = ["Name", "Columns", "Unique"];
        let headers = if show_type && has_details {
            &headers_with_type_and_details[..]
        } else if show_type {
            &headers_with_type[..]
        } else if has_details {
            &headers_without_type_and_details[..]
        } else {
            &headers_without_type[..]
        };
        // Width sampling sees only the first 50 rows, so row_fn rebuilds text
        // per visible row instead of indexing into the sample
        let data_rows: Vec<Vec<String>> = table
            .indexes
            .iter()
            .take(50)
            .map(|index| index_row(index, show_type, has_details))
            .collect();
        let col_widths = calculate_column_widths(headers, &data_rows);
        let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

        use crate::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers,
                widths: &widths,
                total_items: table.indexes.len(),
                empty_message: "No indexes",
            },
            scroll_offset,
            theme,
            |idx| {
                index_row(&table.indexes[idx], show_type, has_details)
                    .into_iter()
                    .map(Cell::from)
                    .collect()
            },
        );
    }

    fn render_foreign_keys(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let headers = ["Name", "Columns", "References"];
        // Width sampling sees only the first 50 rows, so row_fn rebuilds text
        // per visible row instead of indexing into the sample
        let data_rows: Vec<Vec<String>> = table
            .foreign_keys
            .iter()
            .take(50)
            .map(foreign_key_row)
            .collect();
        let col_widths = calculate_column_widths(&headers, &data_rows);
        let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

        use crate::primitives::molecules::{StripedTableConfig, render_striped_table};
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
            theme,
            |idx| {
                foreign_key_row(&table.foreign_keys[idx])
                    .into_iter()
                    .map(Cell::from)
                    .collect()
            },
        );
    }

    fn render_rls(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        match &table.rls {
            None => {
                let msg = Paragraph::new("RLS not enabled")
                    .style(Style::default().fg(theme.semantic.text.placeholder));
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
                            theme.semantic.status.success
                        } else {
                            theme.semantic.status.error
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
                                truncate_to_width(qual, 50)
                            )));
                        }
                    }
                }

                let total_lines = lines.len();
                let visible_lines = area.height as usize;

                use crate::primitives::atoms::scroll_indicator::{
                    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
                };
                let clamped_scroll_offset =
                    clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

                let paragraph = Paragraph::new(lines)
                    .style(Style::default().fg(theme.semantic.text.primary))
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
                        has_horizontal_scrollbar: false,
                    },
                    theme,
                );
            }
        }
    }

    fn render_triggers(
        frame: &mut Frame,
        area: Rect,
        table: &Table,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let headers = ["Name", "Timing", "Event", "Function", "SecDef"];
        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
        ];

        use crate::primitives::molecules::{StripedTableConfig, render_striped_table};
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
            theme,
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
        ddl: String,
        scroll_offset: usize,
        flash_timers: &crate::app::model::shared::flash_timer::FlashTimerStore,
        now: Instant,
        theme: &ThemePalette,
    ) {
        let total_lines = ddl.lines().count();
        let visible_lines = area.height as usize;

        use crate::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
        };
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

        let flash_active = flash_timers.is_active(FlashId::Ddl, now);

        let mut lines: Vec<Line> = ddl
            .lines()
            .map(|l| {
                Line::from(l.to_string()).style(Style::default().fg(theme.semantic.text.primary))
            })
            .collect();

        crate::primitives::atoms::apply_yank_flash(&mut lines, flash_active, theme);

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
                has_horizontal_scrollbar: false,
            },
            theme,
        );
    }
}

fn index_row(index: &Index, show_type: bool, show_details: bool) -> Vec<String> {
    let mut row = vec![index.name.clone(), index.columns.join(", ")];
    if show_type {
        row.push(index_type_label(&index.index_type).unwrap_or_default());
    }
    row.push(if index.is_unique() {
        "✓".to_string()
    } else {
        String::new()
    });
    if show_details {
        row.push(if index.is_partial() {
            "✓".to_string()
        } else {
            String::new()
        });
        row.push(index_detail(index));
    }
    row
}

fn index_detail(index: &Index) -> String {
    if index.needs_source_definition_detail()
        && let Some(definition) = &index.definition
    {
        return definition.clone();
    }

    let mut details = Vec::new();
    if index.has_expression() {
        details.push("expression".to_string());
    }
    if index.has_auxiliary_columns() {
        details.push("auxiliary-columns".to_string());
    }
    if index.has_descending_key() {
        details.push("descending".to_string());
    }
    if index.has_non_binary_collation() {
        details.push("collation".to_string());
    }
    details.join("; ")
}

fn index_type_label(index_type: &IndexType) -> Option<String> {
    match index_type {
        IndexType::Unknown => None,
        _ => Some(index_type.to_string()),
    }
}

fn foreign_key_row(fk: &ForeignKey) -> Vec<String> {
    let references = format!(
        "{}.{}({})",
        fk.to_schema,
        fk.to_table,
        fk.to_columns.join(", ")
    );
    vec![
        fk.name.clone(),
        fk.from_columns.join(", "),
        if fk.is_reference_resolved() {
            references
        } else {
            format!("{references} (unresolved)")
        },
    ]
}

fn calculate_column_widths(headers: &[&str], rows: &[Vec<String>]) -> Vec<u16> {
    use unicode_width::UnicodeWidthStr;

    headers
        .iter()
        .enumerate()
        .map(|(col_idx, header)| {
            let mut max_width = UnicodeWidthStr::width(*header);

            for row in rows.iter().take(50) {
                if let Some(cell) = row.get(col_idx) {
                    max_width = max_width.max(UnicodeWidthStr::width(cell.as_str()));
                }
            }

            let max_width = max_width.min(MAX_COL_WIDTH as usize) as u16;
            (max_width + PADDING).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
        })
        .collect()
}
