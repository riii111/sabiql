use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table as RatatuiTable, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::browse::inspector_view_model::{
    InspectorColumnRow, InspectorEmptyState, InspectorForeignKeyRow, InspectorIndexRow,
    InspectorInfoRow, InspectorLoadState, InspectorRlsRow, InspectorSection, InspectorTriggerRow,
    InspectorViewModel,
};
use crate::app::model::shared::engine_feature_profile::InspectorInfoField;
use crate::app::model::shared::flash_timer::{FlashId, FlashTimerStore};
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::viewport::{
    ColumnWidthConfig, MAX_COL_WIDTH, SelectionContext, ViewportPlan, select_viewport_columns,
    widths_fingerprint,
};
use crate::app::services::AppServices;
use crate::domain::DatabaseType;
use crate::primitives::atoms::{apply_yank_flash, panel_block};
use crate::primitives::utils::text_utils::{
    MIN_COL_WIDTH, PADDING, calculate_header_min_widths, truncate_to_width,
};
use crate::theme::ThemePalette;

pub struct Inspector;

#[derive(Debug, Clone, Copy, Default)]
struct ColumnDisplayOptions(u8);

impl ColumnDisplayOptions {
    const READ_ONLY: u8 = 1 << 0;
    const CHARACTER_SET: u8 = 1 << 1;
    const COLLATION: u8 = 1 << 2;
    const GENERATION: u8 = 1 << 3;

    const fn with(self, flag: u8, enabled: bool) -> Self {
        if enabled { Self(self.0 | flag) } else { self }
    }

    const fn show_read_only(self) -> bool {
        self.0 & Self::READ_ONLY != 0
    }

    const fn show_character_set(self) -> bool {
        self.0 & Self::CHARACTER_SET != 0
    }

    const fn show_collation(self) -> bool {
        self.0 & Self::COLLATION != 0
    }

    const fn show_generation(self) -> bool {
        self.0 & Self::GENERATION != 0
    }
}

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
        let view_model = state.inspector_view_model(services.ddl_generator.as_ref());

        Self::render_tab_bar(frame, tab_area, view_model.active_tab(), state, theme);
        Self::render_content(
            frame,
            content_area,
            state,
            &view_model,
            is_focused,
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
            .active_engine_feature_profile()
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
        view_model: &InspectorViewModel,
        is_focused: bool,
        now: Instant,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let block = panel_block(" [2] Inspector ", is_focused, theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match view_model.load_state() {
            InspectorLoadState::NoTableSelected => {
                frame.render_widget(
                    Paragraph::new("(select a table)")
                        .style(Style::default().fg(theme.semantic.text.placeholder)),
                    inner,
                );
                return ViewportPlan::default();
            }
            InspectorLoadState::Loading => {
                return ViewportPlan::default();
            }
            InspectorLoadState::Error(error) => {
                frame.render_widget(
                    Paragraph::new(format!("Error: {error}"))
                        .style(Style::default().fg(theme.semantic.status.error)),
                    inner,
                );
                return ViewportPlan::default();
            }
            InspectorLoadState::Success => {}
        }

        if let Some(empty_state) = view_model.empty_state() {
            let style = if matches!(empty_state, InspectorEmptyState::NoTableSelected) {
                Style::default().fg(theme.semantic.text.placeholder)
            } else {
                Style::default().fg(theme.semantic.text.primary)
            };
            frame.render_widget(Paragraph::new(empty_state.message()).style(style), inner);
            return ViewportPlan::default();
        }

        if let Some(reason) = view_model.unavailable_reason() {
            frame.render_widget(
                Paragraph::new(reason.message())
                    .style(Style::default().fg(theme.semantic.text.placeholder)),
                inner,
            );
            return ViewportPlan::default();
        }

        match view_model.section() {
            Some(InspectorSection::Info { rows }) => {
                Self::render_info(
                    frame,
                    inner,
                    rows,
                    state.ui.inspector_scroll_offset(),
                    theme,
                );
                ViewportPlan::default()
            }
            Some(InspectorSection::Columns {
                rows,
                show_read_only,
                show_character_set,
                show_collation,
                show_generation,
            }) => {
                let options = ColumnDisplayOptions::default()
                    .with(ColumnDisplayOptions::READ_ONLY, *show_read_only)
                    .with(ColumnDisplayOptions::CHARACTER_SET, *show_character_set)
                    .with(ColumnDisplayOptions::COLLATION, *show_collation)
                    .with(ColumnDisplayOptions::GENERATION, *show_generation);
                Self::render_columns(
                    frame,
                    inner,
                    rows,
                    options,
                    state.ui.inspector_scroll_offset(),
                    state.ui.inspector_horizontal_offset(),
                    state.ui.inspector_viewport_plan(),
                    theme,
                )
            }
            Some(InspectorSection::Indexes {
                rows,
                show_type,
                show_partial,
                show_details,
            }) => {
                Self::render_indexes(
                    frame,
                    inner,
                    rows,
                    *show_type,
                    *show_partial,
                    *show_details,
                    state.ui.inspector_scroll_offset(),
                    theme,
                );
                ViewportPlan::default()
            }
            Some(InspectorSection::ForeignKeys { rows }) => {
                Self::render_foreign_keys(
                    frame,
                    inner,
                    rows,
                    state.ui.inspector_scroll_offset(),
                    theme,
                );
                ViewportPlan::default()
            }
            Some(InspectorSection::Rls { rows }) => {
                Self::render_rls(
                    frame,
                    inner,
                    rows,
                    state.ui.inspector_scroll_offset(),
                    theme,
                );
                ViewportPlan::default()
            }
            Some(InspectorSection::Triggers { rows }) => Self::render_triggers(
                frame,
                inner,
                rows,
                state.session.active_database_type_or_default(),
                state.ui.inspector_scroll_offset(),
                state.ui.inspector_horizontal_offset(),
                theme,
            ),
            Some(InspectorSection::Ddl { rows }) => {
                Self::render_ddl(
                    frame,
                    inner,
                    rows,
                    state.ui.inspector_scroll_offset(),
                    &state.flash_timers,
                    now,
                    theme,
                );
                ViewportPlan::default()
            }
            None => ViewportPlan::default(),
        }
    }

    fn render_info(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorInfoRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let lines: Vec<Line> = rows
            .iter()
            .map(|row| match row {
                InspectorInfoRow::Field { field, value } => {
                    Self::render_info_field(*field, value.as_deref(), theme)
                }
            })
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
        value: Option<&'a str>,
        theme: &ThemePalette,
    ) -> Line<'a> {
        let label = match field {
            InspectorInfoField::Owner => "Owner:   ",
            InspectorInfoField::Comment => "Comment: ",
            InspectorInfoField::RowCount => "Rows:    ",
            InspectorInfoField::Schema => "Schema:  ",
            InspectorInfoField::TableName => "Table:   ",
            InspectorInfoField::TableKind => "Kind:    ",
            InspectorInfoField::TableFlags => "Flags:   ",
            InspectorInfoField::Engine => "Engine:  ",
            InspectorInfoField::RowFormat => "Row format: ",
            InspectorInfoField::TableCollation => "Collation:  ",
            InspectorInfoField::CreateOptions => "Create options: ",
        };
        let value = value.map_or_else(
            || {
                Span::styled(
                    "(none)",
                    Style::default().fg(theme.semantic.text.placeholder),
                )
            },
            Span::raw,
        );
        Line::from(vec![Self::info_label(label), value])
    }

    fn info_label(label: &'static str) -> Span<'static> {
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD))
    }

    fn render_columns(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorColumnRow],
        options: ColumnDisplayOptions,
        scroll_offset: usize,
        horizontal_offset: usize,
        stored_plan: &ViewportPlan,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let available_width = area.width.saturating_sub(2);
        let mut headers = vec!["Name", "Type", "Null", "PK"];
        if options.show_read_only() {
            headers.push("Read-only");
        }
        headers.extend(["Default", "Comment"]);
        if options.show_character_set() {
            headers.push("Charset");
        }
        if options.show_collation() {
            headers.push("Collation");
        }
        if options.show_generation() {
            headers.push("Generation");
        }

        let data_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| column_row_cells(row, options))
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

        let render_rows: Vec<Row> = rows
            .iter()
            .enumerate()
            .skip(clamped_scroll_offset)
            .take(data_rows_visible)
            .map(|(row_idx, row)| {
                let cells = column_row_cells(row, options);
                let base_style = if (row_idx - clamped_scroll_offset) % 2 == 1 {
                    Style::default().bg(theme.component.table.striped_row_bg)
                } else {
                    Style::default()
                };

                Row::new(viewport_indices.iter().zip(viewport_widths.iter()).map(
                    |(&col_idx, &col_width)| {
                        let text = cells.get(col_idx).map_or("", String::as_str);
                        let display = truncate_to_width(text, col_width as usize);

                        let read_only_col_idx = options.show_read_only().then_some(4);
                        let comment_col_idx = if options.show_read_only() { 6 } else { 5 };
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

        let table_widget = RatatuiTable::new(render_rows, widths)
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
                label: "col",
            },
            theme,
        );

        plan
    }

    fn render_indexes(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorIndexRow],
        show_type: bool,
        show_partial: bool,
        has_details: bool,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let headers = index_headers(show_type, show_partial, has_details);
        // Width sampling sees only the first 50 rows, so row_fn rebuilds text
        // per visible row instead of indexing into the sample
        let data_rows: Vec<Vec<String>> = rows
            .iter()
            .take(50)
            .map(|row| index_row_cells(row, show_type, show_partial, has_details))
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
                total_items: rows.len(),
                empty_message: "No indexes",
            },
            scroll_offset,
            theme,
            |idx| {
                index_row_cells(&rows[idx], show_type, show_partial, has_details)
                    .into_iter()
                    .map(Cell::from)
                    .collect()
            },
        );
    }

    fn render_foreign_keys(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorForeignKeyRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let headers = ["Name", "Columns", "References", "On update", "On delete"];
        // Width sampling sees only the first 50 rows, so row_fn rebuilds text
        // per visible row instead of indexing into the sample
        let data_rows: Vec<Vec<String>> = rows.iter().take(50).map(foreign_key_row_cells).collect();
        let col_widths = calculate_column_widths(&headers, &data_rows);
        let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

        use crate::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers: &headers,
                widths: &widths,
                total_items: rows.len(),
                empty_message: "No foreign keys",
            },
            scroll_offset,
            theme,
            |idx| {
                foreign_key_row_cells(&rows[idx])
                    .into_iter()
                    .map(Cell::from)
                    .collect()
            },
        );
    }

    fn render_rls(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorRlsRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let mut lines = Vec::with_capacity(rows.len());
        for row in rows {
            match row {
                InspectorRlsRow::RlsStatus { enabled, force } => {
                    let status = if *enabled {
                        if *force { "Enabled (FORCE)" } else { "Enabled" }
                    } else {
                        "Disabled"
                    };
                    lines.push(Line::from(vec![
                        Span::raw("Status: "),
                        Span::styled(
                            status,
                            Style::default().fg(if *enabled {
                                theme.semantic.status.success
                            } else {
                                theme.semantic.status.error
                            }),
                        ),
                    ]));
                }
                InspectorRlsRow::RlsSpacer => lines.push(Line::from("")),
                InspectorRlsRow::RlsPoliciesHeading => lines.push(Line::from(Span::styled(
                    "Policies:",
                    Style::default().add_modifier(Modifier::BOLD),
                ))),
                InspectorRlsRow::RlsPolicy {
                    name,
                    command,
                    permissive,
                } => lines.push(Line::from(format!(
                    "  {} ({}) - {}",
                    name,
                    command,
                    if *permissive {
                        "PERMISSIVE"
                    } else {
                        "RESTRICTIVE"
                    }
                ))),
                InspectorRlsRow::RlsPolicyQual(qual) => lines.push(Line::from(format!(
                    "    USING: {}",
                    truncate_to_width(qual, 50)
                ))),
            }
        }

        let total_lines = lines.len();
        let visible_lines = area.height as usize;

        use crate::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
        };
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

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

    fn render_triggers(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorTriggerRow],
        database_type: DatabaseType,
        scroll_offset: usize,
        horizontal_offset: usize,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let (headers, widths): (&[&str], &[Constraint]) = match database_type {
            DatabaseType::PostgreSQL => (
                &["Name", "Timing", "Event", "Function", "Security"],
                &[
                    Constraint::Percentage(25),
                    Constraint::Percentage(15),
                    Constraint::Percentage(20),
                    Constraint::Percentage(25),
                    Constraint::Percentage(15),
                ],
            ),
            DatabaseType::SQLite => (
                &["Name", "Timing", "Event", "Definition"],
                &[
                    Constraint::Percentage(25),
                    Constraint::Percentage(15),
                    Constraint::Percentage(20),
                    Constraint::Percentage(40),
                ],
            ),
            DatabaseType::MySQL => {
                return Self::render_mysql_trigger_details(
                    frame,
                    area,
                    rows,
                    scroll_offset,
                    horizontal_offset,
                    theme,
                );
            }
        };

        use crate::primitives::molecules::{StripedTableConfig, render_striped_table};
        render_striped_table(
            frame,
            area,
            &StripedTableConfig {
                headers,
                widths,
                total_items: rows.len(),
                empty_message: "No triggers",
            },
            scroll_offset,
            theme,
            |idx| {
                trigger_row_cells(&rows[idx], database_type)
                    .into_iter()
                    .map(Cell::from)
                    .collect()
            },
        );

        ViewportPlan::default()
    }

    fn render_mysql_trigger_details(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorTriggerRow],
        scroll_offset: usize,
        horizontal_offset: usize,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        use crate::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
        };

        let lines: Vec<Line> = rows
            .iter()
            .flat_map(mysql_trigger_detail_lines)
            .map(|line| Line::from(line).style(Style::default().fg(theme.semantic.text.primary)))
            .collect();
        let total_lines = lines.len();
        let has_vertical_scrollbar = total_lines > area.height as usize;
        let content_area = Rect {
            width: area.width.saturating_sub(u16::from(has_vertical_scrollbar)),
            ..area
        };
        let visible_lines = content_area.height as usize;
        let content_width = lines.iter().map(Line::width).max().unwrap_or_default();
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);
        let clamped_horizontal_offset = clamp_scroll_offset(
            horizontal_offset,
            content_area.width as usize,
            content_width,
        );

        frame.render_widget(
            Paragraph::new(lines).scroll((
                clamped_scroll_offset.min(u16::MAX as usize) as u16,
                clamped_horizontal_offset.min(u16::MAX as usize) as u16,
            )),
            content_area,
        );

        if has_vertical_scrollbar {
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

        ViewportPlan {
            column_count: 1,
            max_offset: content_width.saturating_sub(content_area.width as usize),
            total_columns: 1,
            available_width: content_area.width,
            widths_fingerprint: 0,
        }
    }

    fn render_ddl(
        frame: &mut Frame,
        area: Rect,
        rows: &[String],
        scroll_offset: usize,
        flash_timers: &FlashTimerStore,
        now: Instant,
        theme: &ThemePalette,
    ) {
        let total_lines = rows.len();
        let visible_lines = area.height as usize;

        use crate::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
        };
        let clamped_scroll_offset = clamp_scroll_offset(scroll_offset, visible_lines, total_lines);

        let flash_active = flash_timers.is_active(FlashId::Ddl, now);

        let mut lines: Vec<Line> = rows
            .iter()
            .map(|line| {
                Line::from(line.clone()).style(Style::default().fg(theme.semantic.text.primary))
            })
            .collect();

        apply_yank_flash(&mut lines, flash_active, theme);

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

fn index_headers(show_type: bool, show_partial: bool, has_details: bool) -> Vec<&'static str> {
    let mut headers = vec!["Name", "Columns"];
    if show_type {
        headers.push("Type");
    }
    headers.push("Unique");
    if show_partial && has_details {
        headers.push("Partial");
    }
    if has_details {
        headers.push("Detail");
    }
    headers
}

fn column_row_cells(row: &InspectorColumnRow, options: ColumnDisplayOptions) -> Vec<String> {
    let mut cells = vec![
        row.name.clone(),
        row.data_type.clone(),
        checkmark(row.nullable),
        checkmark(row.primary_key),
    ];
    if options.show_read_only() {
        cells.push(row.read_only_reason.clone().unwrap_or_default());
    }
    cells.push(row.default.clone().unwrap_or_default());
    cells.push(row.comment.clone().unwrap_or_default());
    if options.show_character_set() {
        cells.push(row.character_set_name.clone().unwrap_or_default());
    }
    if options.show_collation() {
        cells.push(row.collation_name.clone().unwrap_or_default());
    }
    if options.show_generation() {
        cells.push(generation_display(row));
    }
    cells
}

fn generation_display(row: &InspectorColumnRow) -> String {
    match (row.generation_kind, row.generation_expression.as_deref()) {
        (Some(kind), Some(expression)) => format!("{}: {expression}", kind.label()),
        (Some(kind), None) => kind.label().to_string(),
        (None, Some(expression)) => expression.to_string(),
        (None, None) => String::new(),
    }
}

fn index_row_cells(
    row: &InspectorIndexRow,
    show_type: bool,
    show_partial: bool,
    show_details: bool,
) -> Vec<String> {
    let mut cells = vec![row.name.clone(), row.columns.clone()];
    if show_type {
        cells.push(row.index_type.clone().unwrap_or_default());
    }
    cells.push(checkmark(row.unique));
    if show_partial && show_details {
        cells.push(checkmark(row.partial));
    }
    if show_details {
        cells.push(row.detail.clone().unwrap_or_default());
    }
    cells
}

fn foreign_key_row_cells(row: &InspectorForeignKeyRow) -> Vec<String> {
    vec![
        row.name.clone(),
        row.columns.clone(),
        row.references.clone(),
        row.on_update.clone(),
        row.on_delete.clone(),
    ]
}

fn trigger_row_cells(row: &InspectorTriggerRow, database_type: DatabaseType) -> Vec<String> {
    let mut cells = Vec::new();
    if database_type == DatabaseType::MySQL {
        cells.push(
            row.action_order
                .map(|order| order.to_string())
                .unwrap_or_default(),
        );
    }
    cells.extend([
        row.name.clone(),
        row.timing.clone(),
        row.events.clone(),
        row.definition.clone(),
    ]);
    if database_type != DatabaseType::SQLite {
        cells.push(row.security_context.clone().unwrap_or_default());
    }
    cells
}

fn mysql_trigger_detail_lines(row: &InspectorTriggerRow) -> Vec<String> {
    let context = row.creation_context.as_ref();
    vec![
        format!(
            "Order: {}",
            row.action_order
                .map(|order| order.to_string())
                .unwrap_or_default()
        ),
        format!("Name: {}", row.name),
        format!("Timing: {}", row.timing),
        format!("Event: {}", row.events),
        format!("Action: {}", row.definition),
        format!(
            "Definer: {}",
            row.security_context.as_deref().unwrap_or_default()
        ),
        format!(
            "SQL_MODE: {}",
            context
                .and_then(|context| context.sql_mode.as_deref())
                .unwrap_or_default()
        ),
        format!(
            "CHARACTER_SET_CLIENT: {}",
            context
                .and_then(|context| context.character_set_client.as_deref())
                .unwrap_or_default()
        ),
        format!(
            "COLLATION_CONNECTION: {}",
            context
                .and_then(|context| context.collation_connection.as_deref())
                .unwrap_or_default()
        ),
        format!(
            "DATABASE_COLLATION: {}",
            context
                .and_then(|context| context.database_collation.as_deref())
                .unwrap_or_default()
        ),
        format!(
            "CREATED: {}",
            context
                .and_then(|context| context.created.as_deref())
                .unwrap_or_default()
        ),
        String::new(),
    ]
}

fn checkmark(value: bool) -> String {
    if value {
        "✓".to_string()
    } else {
        String::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TriggerCreationContext;

    #[test]
    fn index_headers_match_row_cells_for_all_flag_combinations() {
        let row = InspectorIndexRow {
            name: "idx_users_email".to_string(),
            columns: "email".to_string(),
            index_type: Some("B-tree".to_string()),
            unique: false,
            partial: true,
            detail: Some("CREATE INDEX idx_users_email ON users(email)".to_string()),
        };

        let cases = [
            (
                (false, false, false),
                vec!["Name", "Columns", "Unique"],
                vec!["idx_users_email", "email", ""],
            ),
            (
                (true, false, false),
                vec!["Name", "Columns", "Type", "Unique"],
                vec!["idx_users_email", "email", "B-tree", ""],
            ),
            (
                (false, true, false),
                vec!["Name", "Columns", "Unique"],
                vec!["idx_users_email", "email", ""],
            ),
            (
                (true, true, false),
                vec!["Name", "Columns", "Type", "Unique"],
                vec!["idx_users_email", "email", "B-tree", ""],
            ),
            (
                (false, false, true),
                vec!["Name", "Columns", "Unique", "Detail"],
                vec![
                    "idx_users_email",
                    "email",
                    "",
                    "CREATE INDEX idx_users_email ON users(email)",
                ],
            ),
            (
                (true, false, true),
                vec!["Name", "Columns", "Type", "Unique", "Detail"],
                vec![
                    "idx_users_email",
                    "email",
                    "B-tree",
                    "",
                    "CREATE INDEX idx_users_email ON users(email)",
                ],
            ),
            (
                (false, true, true),
                vec!["Name", "Columns", "Unique", "Partial", "Detail"],
                vec![
                    "idx_users_email",
                    "email",
                    "",
                    "✓",
                    "CREATE INDEX idx_users_email ON users(email)",
                ],
            ),
            (
                (true, true, true),
                vec!["Name", "Columns", "Type", "Unique", "Partial", "Detail"],
                vec![
                    "idx_users_email",
                    "email",
                    "B-tree",
                    "",
                    "✓",
                    "CREATE INDEX idx_users_email ON users(email)",
                ],
            ),
        ];

        for ((show_type, show_partial, has_details), expected_headers, expected_cells) in cases {
            let headers = index_headers(show_type, show_partial, has_details);
            let cells = index_row_cells(&row, show_type, show_partial, has_details);

            assert_eq!(headers, expected_headers);
            assert_eq!(
                cells,
                expected_cells
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn foreign_key_row_cells_include_referential_actions() {
        let row = InspectorForeignKeyRow {
            name: "fk_users_department".to_string(),
            columns: "department_id".to_string(),
            references: "public.departments(id)".to_string(),
            on_update: "NO ACTION".to_string(),
            on_delete: "CASCADE".to_string(),
        };

        assert_eq!(
            foreign_key_row_cells(&row),
            vec![
                "fk_users_department",
                "department_id",
                "public.departments(id)",
                "NO ACTION",
                "CASCADE",
            ]
        );
    }

    #[test]
    fn mysql_trigger_details_render_all_creation_context_fields() {
        use crate::app::model::shared::theme_id::ThemeId;
        use crate::theme::palette_for;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::layout::Rect;

        let row = InspectorTriggerRow {
            name: "audit_changes".to_string(),
            timing: "BEFORE".to_string(),
            events: "UPDATE".to_string(),
            action_order: Some(2),
            definition: format!("SET NEW.value = {}", "x".repeat(100)),
            security_context: Some("sabiql@%".to_string()),
            creation_context: Some(TriggerCreationContext {
                sql_mode: Some("STRICT_TRANS_TABLES".to_string()),
                character_set_client: Some("utf8mb4".to_string()),
                collation_connection: Some("utf8mb4_0900_ai_ci".to_string()),
                database_collation: Some("utf8mb4_0900_ai_ci".to_string()),
                created: Some("2026-08-21 10:20:30.00".to_string()),
            }),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        let theme = palette_for(ThemeId::Default);
        let mut plan = ViewportPlan::default();

        terminal
            .draw(|frame| {
                plan = Inspector::render_triggers(
                    frame,
                    Rect::new(0, 0, 80, 12),
                    &[row],
                    DatabaseType::MySQL,
                    0,
                    0,
                    theme,
                );
            })
            .unwrap();

        assert!(plan.max_offset > 0);

        let buffer = terminal.backend().buffer();
        let rendered: String = (0..buffer.area.height)
            .flat_map(|y| {
                (0..buffer.area.width)
                    .map(move |x| buffer.cell((x, y)).unwrap().symbol())
                    .chain(std::iter::once("\n"))
            })
            .collect();

        for expected in [
            "SQL_MODE: STRICT_TRANS_TABLES",
            "CHARACTER_SET_CLIENT: utf8mb4",
            "COLLATION_CONNECTION: utf8mb4_0900_ai_ci",
            "DATABASE_COLLATION: utf8mb4_0900_ai_ci",
            "CREATED: 2026-08-21 10:20:30.00",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected:?} in {rendered:?}"
            );
        }
    }
}
