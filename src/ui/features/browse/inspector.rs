use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table as RatatuiTable, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::engine_feature_profile::InspectorInfoField;
use crate::app::model::shared::flash_timer::{FlashId, FlashTimerStore};
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::inspector_view_model::{
    InspectorDisplayRow, InspectorEmptyState, InspectorSection, InspectorViewModel,
};
use crate::app::model::shared::viewport::{
    ColumnWidthConfig, MAX_COL_WIDTH, SelectionContext, ViewportPlan, select_viewport_columns,
    widths_fingerprint,
};
use crate::app::services::AppServices;
use crate::primitives::atoms::{apply_yank_flash, panel_block};
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

        match view_model.sections().first() {
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
            }) => Self::render_columns(
                frame,
                inner,
                rows,
                *show_read_only,
                state.ui.inspector_scroll_offset(),
                state.ui.inspector_horizontal_offset(),
                state.ui.inspector_viewport_plan(),
                theme,
            ),
            Some(InspectorSection::Indexes {
                rows,
                show_type,
                show_details,
            }) => {
                Self::render_indexes(
                    frame,
                    inner,
                    rows,
                    *show_type,
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
            Some(InspectorSection::Triggers { rows }) => {
                Self::render_triggers(
                    frame,
                    inner,
                    rows,
                    state.ui.inspector_scroll_offset(),
                    theme,
                );
                ViewportPlan::default()
            }
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
        rows: &[InspectorDisplayRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let lines: Vec<Line> = rows
            .iter()
            .filter_map(|row| match row {
                InspectorDisplayRow::Info { field, value } => {
                    Some(Self::render_info_field(*field, value.as_deref(), theme))
                }
                _ => None,
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
        rows: &[InspectorDisplayRow],
        show_read_only: bool,
        scroll_offset: usize,
        horizontal_offset: usize,
        stored_plan: &ViewportPlan,
        theme: &ThemePalette,
    ) -> ViewportPlan {
        let available_width = area.width.saturating_sub(2);
        let mut headers = vec!["Name", "Type", "Null", "PK"];
        if show_read_only {
            headers.push("Read-only");
        }
        headers.extend(["Default", "Comment"]);

        let data_rows: Vec<Vec<String>> = rows
            .iter()
            .filter_map(|row| match row {
                InspectorDisplayRow::Cells(cells) => Some(cells.clone()),
                _ => None,
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
                label: "col",
            },
            theme,
        );

        plan
    }

    fn render_indexes(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorDisplayRow],
        show_type: bool,
        has_details: bool,
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
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
        let data_rows: Vec<Vec<String>> = rows
            .iter()
            .take(50)
            .filter_map(|row| match row {
                InspectorDisplayRow::Cells(cells) => Some(cells.clone()),
                _ => None,
            })
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
                total_items: rows.len(),
                empty_message: "No indexes",
            },
            scroll_offset,
            theme,
            |idx| match &rows[idx] {
                InspectorDisplayRow::Cells(cells) => {
                    cells.iter().cloned().map(Cell::from).collect()
                }
                _ => Vec::new(),
            },
        );
    }

    fn render_foreign_keys(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorDisplayRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let headers = ["Name", "Columns", "References"];
        // Width sampling sees only the first 50 rows, so row_fn rebuilds text
        // per visible row instead of indexing into the sample
        let data_rows: Vec<Vec<String>> = rows
            .iter()
            .take(50)
            .filter_map(|row| match row {
                InspectorDisplayRow::Cells(cells) => Some(cells.clone()),
                _ => None,
            })
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
                empty_message: "No foreign keys",
            },
            scroll_offset,
            theme,
            |idx| match &rows[idx] {
                InspectorDisplayRow::Cells(cells) => {
                    cells.iter().cloned().map(Cell::from).collect()
                }
                _ => Vec::new(),
            },
        );
    }

    fn render_rls(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorDisplayRow],
        scroll_offset: usize,
        theme: &ThemePalette,
    ) {
        let mut lines = Vec::with_capacity(rows.len());
        for row in rows {
            match row {
                InspectorDisplayRow::RlsStatus { enabled, force } => {
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
                InspectorDisplayRow::RlsSpacer => lines.push(Line::from("")),
                InspectorDisplayRow::RlsPoliciesHeading => lines.push(Line::from(Span::styled(
                    "Policies:",
                    Style::default().add_modifier(Modifier::BOLD),
                ))),
                InspectorDisplayRow::RlsPolicy {
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
                InspectorDisplayRow::RlsPolicyQual(qual) => lines.push(Line::from(format!(
                    "    USING: {}",
                    truncate_to_width(qual, 50)
                ))),
                _ => {}
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
        rows: &[InspectorDisplayRow],
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
                total_items: rows.len(),
                empty_message: "No triggers",
            },
            scroll_offset,
            theme,
            |idx| match &rows[idx] {
                InspectorDisplayRow::Cells(cells) => {
                    cells.iter().cloned().map(Cell::from).collect()
                }
                _ => Vec::new(),
            },
        );
    }

    fn render_ddl(
        frame: &mut Frame,
        area: Rect,
        rows: &[InspectorDisplayRow],
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
            .filter_map(|row| match row {
                InspectorDisplayRow::Text(line) => Some(
                    Line::from(line.clone())
                        .style(Style::default().fg(theme.semantic.text.primary)),
                ),
                _ => None,
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
