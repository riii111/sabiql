use std::collections::BTreeSet;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, Wrap};

use crate::primitives::atoms::{panel_block_highlight, text_cursor_spans};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::ui_state::{RESULT_INNER_OVERHEAD, ResultSelection, YankFlash};
use crate::app::model::shared::viewport::{
    ColumnWidthConfig, ColumnWidthsCache, MAX_COL_WIDTH, SelectionContext, ViewportPlan,
    select_viewport_columns, widths_fingerprint,
};
use crate::app::model::shared::wrapped_cell::{self as wrapped_cell_layout, WrappedCellSettings};
use crate::domain::{QueryResult, QuerySource};
use crate::primitives::utils::text_utils::{
    MIN_COL_WIDTH, PADDING, calculate_header_min_widths, truncate_to_width,
};
use crate::theme::ThemePalette;

pub struct ResultPane;

/// Geometry the result pane measures during a draw and writes back to the
/// model. Replaces a growing tuple return from `ResultPane::render`.
pub struct RenderedResultGeometry {
    pub plan: ViewportPlan,
    pub widths_cache: ColumnWidthsCache,
    /// Corrected/clamped `cell_vertical_offset` for the active cell.
    pub cell_vertical_offset: usize,
    /// Wrapped Cell Mode layout (per-row line heights), or `None` when the mode
    /// is off. Drives line-based scroll math in the reducer.
    pub wrapped_cell_layout: Option<wrapped_cell_layout::MeasuredWrappedCellLayout>,
}

struct EditingCellView<'a> {
    row: usize,
    col: usize,
    draft: &'a str,
    actively_editing: bool,
    cursor: usize,
}

struct ResultTableParams<'a> {
    scroll_offset: usize,
    horizontal_offset: usize,
    stored_plan: &'a ViewportPlan,
    stored_cache: &'a ColumnWidthsCache,
    result_generation: u64,
    selection: &'a ResultSelection,
    editing_cell: Option<EditingCellView<'a>>,
    staged_delete_rows: &'a BTreeSet<usize>,
    yank_flash: Option<YankFlash>,
    cell_vertical_offset: usize,
    now: Instant,
    effective_wrapped_cell: WrappedCellSettings,
    wrapped_cell_enabled: bool,
    /// Wrapped Cell Mode layout measured on the previous frame, reused when its
    /// key still matches so per-row heights are not re-measured every draw.
    stored_wrapped_cell: Option<&'a wrapped_cell_layout::MeasuredWrappedCellLayout>,
}

impl ResultPane {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        now: Instant,
        theme: &ThemePalette,
    ) -> RenderedResultGeometry {
        let is_focused = state.ui.focused_pane == FocusedPane::Result;
        let should_highlight = state
            .query
            .result_highlight_until()
            .is_some_and(|t| now < t);

        let result = state.query.visible_result();
        let title = Self::build_title(result);

        let block = panel_block_highlight(&title, is_focused, should_highlight, theme);

        let default_result = || RenderedResultGeometry {
            plan: ViewportPlan::default(),
            widths_cache: ColumnWidthsCache::default(),
            cell_vertical_offset: 0,
            wrapped_cell_layout: None,
        };

        if let Some(result) = result {
            if result.is_error() {
                Self::render_error(frame, area, result, block, theme);
                default_result()
            } else if result.rows.is_empty() {
                Self::render_empty(frame, area, block, theme);
                default_result()
            } else {
                let cell_edit = state.result_interaction.cell_edit();
                let editing_cell = cell_edit.is_active().then(|| EditingCellView {
                    row: cell_edit.row.unwrap_or_default(),
                    col: cell_edit.col.unwrap_or_default(),
                    draft: cell_edit.draft_value(),
                    actively_editing: state.input_mode()
                        == crate::app::model::shared::input_mode::InputMode::CellEdit,
                    cursor: cell_edit.input.cursor(),
                });
                Self::render_table(
                    frame,
                    area,
                    result,
                    block,
                    ResultTableParams {
                        scroll_offset: state.result_interaction.scroll_offset,
                        horizontal_offset: state.result_interaction.horizontal_offset,
                        stored_plan: &state.ui.result_viewport_plan,
                        stored_cache: &state.ui.result_widths_cache,
                        result_generation: state.query.result_generation(),
                        selection: state.result_interaction.selection(),
                        editing_cell,
                        staged_delete_rows: state.result_interaction.staged_delete_rows(),
                        yank_flash: state.result_interaction.yank_flash,
                        cell_vertical_offset: state.result_interaction.cell_vertical_offset,
                        now,
                        effective_wrapped_cell: state.ui.effective_wrapped_cell(),
                        wrapped_cell_enabled: state.ui.wrapped_cell_enabled,
                        stored_wrapped_cell: state.ui.result_wrapped_cell_layout.as_ref(),
                    },
                    theme,
                )
            }
        } else {
            Self::render_placeholder(frame, area, block, theme);
            default_result()
        }
    }

    fn build_title(result: Option<&QueryResult>) -> String {
        match result {
            None => " [3] Result ".to_string(),
            Some(r) => {
                let name = match r.source {
                    QuerySource::Preview => "Result",
                    QuerySource::Adhoc => "Result Query",
                };

                if r.is_error() {
                    format!(" [3] {name} ERROR ")
                } else {
                    format!(
                        " [3] {} ({}, {}ms) ",
                        name,
                        r.row_count_display(),
                        r.execution_time_ms,
                    )
                }
            }
        }
    }

    fn render_placeholder(frame: &mut Frame, area: Rect, block: Block, theme: &ThemePalette) {
        let content = Paragraph::new("(select a table to preview)")
            .block(block)
            .style(Style::default().fg(theme.semantic.text.placeholder));
        frame.render_widget(content, area);
    }

    fn render_empty(frame: &mut Frame, area: Rect, block: Block, theme: &ThemePalette) {
        let content = Paragraph::new("No rows returned")
            .block(block)
            .style(Style::default().fg(theme.semantic.text.placeholder));
        frame.render_widget(content, area);
    }

    fn render_error(
        frame: &mut Frame,
        area: Rect,
        result: &QueryResult,
        block: Block,
        theme: &ThemePalette,
    ) {
        let error_msg = result.error.as_deref().unwrap_or("Unknown error");

        let block = block.style(Style::default().fg(theme.semantic.status.error));

        let content = Paragraph::new(error_msg)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme.semantic.status.error));

        frame.render_widget(content, area);
    }

    fn render_table(
        frame: &mut Frame,
        area: Rect,
        result: &QueryResult,
        block: Block,
        params: ResultTableParams,
        theme: &ThemePalette,
    ) -> RenderedResultGeometry {
        let ResultTableParams {
            scroll_offset,
            horizontal_offset,
            stored_plan,
            stored_cache,
            result_generation,
            selection,
            editing_cell,
            staged_delete_rows,
            yank_flash,
            cell_vertical_offset,
            now,
            effective_wrapped_cell,
            wrapped_cell_enabled,
            stored_wrapped_cell,
        } = params;
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if result.columns.is_empty() {
            return RenderedResultGeometry {
                plan: ViewportPlan::default(),
                widths_cache: ColumnWidthsCache::default(),
                cell_vertical_offset: 0,
                wrapped_cell_layout: None,
            };
        }

        let cached = stored_cache.is_valid(result_generation);
        let fresh_ideal;
        let fresh_min;
        let (ideal_widths, min_widths) = if cached {
            (
                &stored_cache.ideal_widths[..],
                &stored_cache.header_min_widths[..],
            )
        } else {
            fresh_ideal = calculate_ideal_widths(&result.columns, &result.rows);
            fresh_min = calculate_header_min_widths(&result.columns);
            (&fresh_ideal[..], &fresh_min[..])
        };

        let fingerprint = widths_fingerprint(ideal_widths, min_widths);
        let plan = if stored_plan.needs_recalculation(inner.width, fingerprint) {
            ViewportPlan::calculate(ideal_widths, min_widths, inner.width)
        } else {
            stored_plan.clone()
        };

        let mut widths_cache = if cached {
            stored_cache.clone()
        } else {
            ColumnWidthsCache::new(
                ideal_widths.to_vec(),
                min_widths.to_vec(),
                result_generation,
            )
        };

        // `ideal_widths` is based on first lines only (normal view truncates to
        // one line). Wrapped Cell wraps the full cell, so width must account for
        // the widest line — otherwise jsonb output starting with `{` gets squeezed.
        let wrapped_cell_widths = if wrapped_cell_enabled {
            cached_wrapped_cell_ideal_widths(&mut widths_cache, &result.columns, &result.rows)
        } else {
            ideal_widths
        };

        let effective = effective_wrapped_cell;
        if wrapped_cell_enabled {
            let wrapped_cell_key = wrapped_cell_layout::WrappedCellLayoutKey {
                result_generation,
                inner_width: inner.width,
                allow_horizontal_scroll: effective.allow_horizontal_scroll,
                max_lines_per_row: effective.max_lines_per_row,
                viewport_fingerprint: 0,
            };
            let use_scrollable = effective.allow_horizontal_scroll
                || needs_wrapped_cell_horizontal_scroll(result.columns.len(), inner.width);
            let (plan, cell_vertical_offset, measured_layout) = if use_scrollable {
                Self::render_wrapped_cell_table_scrollable(
                    frame,
                    inner,
                    result,
                    wrapped_cell_widths,
                    min_widths,
                    horizontal_offset,
                    effective,
                    scroll_offset,
                    selection,
                    editing_cell.as_ref(),
                    staged_delete_rows,
                    yank_flash,
                    cell_vertical_offset,
                    now,
                    stored_wrapped_cell,
                    wrapped_cell_key,
                    theme,
                )
            } else {
                Self::render_wrapped_cell_table(
                    frame,
                    inner,
                    result,
                    wrapped_cell_widths,
                    effective,
                    scroll_offset,
                    selection,
                    editing_cell.as_ref(),
                    staged_delete_rows,
                    yank_flash,
                    cell_vertical_offset,
                    now,
                    stored_wrapped_cell,
                    wrapped_cell_key,
                    theme,
                )
            };
            return RenderedResultGeometry {
                plan,
                widths_cache,
                cell_vertical_offset,
                wrapped_cell_layout: Some(measured_layout),
            };
        }

        let clamped_offset = horizontal_offset.min(plan.max_offset);

        let config = ColumnWidthConfig {
            ideal_widths,
            min_widths,
        };
        let ctx = SelectionContext {
            horizontal_offset: clamped_offset,
            available_width: inner.width,
            fixed_count: Some(plan.column_count),
            max_offset: plan.max_offset,
        };
        let (viewport_indices, viewport_widths) = select_viewport_columns(&config, &ctx);

        if viewport_indices.is_empty() {
            return RenderedResultGeometry {
                plan,
                widths_cache,
                cell_vertical_offset: 0,
                wrapped_cell_layout: None,
            };
        }

        let widths: Vec<Constraint> = viewport_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let col_name = result.columns.get(idx).map_or("", String::as_str);
            Cell::from(col_name.to_string())
        }))
        .style(
            Style::default()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD)
                .fg(theme.semantic.text.primary),
        )
        .height(1);

        let data_rows_visible = inner.height.saturating_sub(RESULT_INNER_OVERHEAD) as usize;
        let scroll_viewport_size = data_rows_visible;
        let active_row = selection.row();
        let active_cell = selection.cell();

        let yank_flash_active = yank_flash.is_some_and(|f| now < f.until);

        let rows: Vec<Row> = result
            .rows
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(data_rows_visible)
            .map(|(abs_row_idx, row)| {
                let is_staged_for_delete = staged_delete_rows.contains(&abs_row_idx);
                let is_active_row = active_row == Some(abs_row_idx);
                let flash_scope = yank_flash
                    .filter(|f| yank_flash_active && f.row == abs_row_idx)
                    .map(|f| f.col);
                let is_row_flash = flash_scope == Some(None);
                let row_bg = if is_row_flash {
                    Some(theme.component.feedback.yank_flash_bg)
                } else if is_staged_for_delete {
                    Some(theme.component.table.staged_delete_bg)
                } else if is_active_row {
                    Some(theme.component.table.result_row_active_bg)
                } else if (abs_row_idx - scroll_offset) % 2 == 1 {
                    Some(theme.component.table.striped_row_bg)
                } else {
                    None
                };

                let cells: Vec<Cell> = viewport_indices
                    .iter()
                    .zip(viewport_widths.iter())
                    .map(|(&orig_idx, &col_width)| {
                        let val = row.get(orig_idx).map_or("", String::as_str);
                        let is_editing_cell = editing_cell
                            .as_ref()
                            .is_some_and(|e| e.row == abs_row_idx && e.col == orig_idx);
                        let mut cell = if let Some(e) = &editing_cell
                            && is_editing_cell
                        {
                            render_editing_cell(e, col_width, theme)
                        } else {
                            let display = truncate_cell(val, col_width as usize);
                            Cell::from(display)
                        };
                        if !is_editing_cell {
                            if is_row_flash || flash_scope == Some(Some(orig_idx)) {
                                cell = cell.style(
                                    Style::default()
                                        .fg(theme.component.feedback.yank_flash_fg)
                                        .bg(theme.component.feedback.yank_flash_bg),
                                );
                            } else if is_staged_for_delete {
                                cell = cell.style(
                                    Style::default().fg(theme.component.table.staged_delete_fg),
                                );
                            } else if is_active_row && active_cell == Some(orig_idx) {
                                cell = cell.style(
                                    Style::default()
                                        .bg(theme.component.table.result_cell_active_bg),
                                );
                            }
                        }
                        cell
                    })
                    .collect();

                let mut r = Row::new(cells);
                if let Some(bg) = row_bg {
                    r = r.style(Style::default().bg(bg));
                }
                r
            })
            .collect();

        let table = Table::new(rows, widths)
            .header(header)
            .style(Style::default().fg(theme.semantic.text.primary));

        frame.render_widget(table, inner);

        let total_rows = result.rows.len();
        let total_cols = result.columns.len();

        use crate::primitives::atoms::scroll_indicator::{
            HorizontalScrollParams, VerticalScrollParams, render_horizontal_scroll_indicator,
            render_vertical_scroll_indicator_bar,
        };
        let has_h_scroll = plan.has_horizontal_scroll();
        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: scroll_viewport_size,
                total_items: total_rows,
                has_horizontal_scrollbar: has_h_scroll,
            },
            theme,
        );
        render_horizontal_scroll_indicator(
            frame,
            inner,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: plan.indicator_viewport_size(),
                total_items: total_cols,
                label: "col",
            },
            theme,
        );

        RenderedResultGeometry {
            plan,
            widths_cache,
            cell_vertical_offset: 0,
            wrapped_cell_layout: None,
        }
    }

    /// Compute the row background color based on row state (flash, staged,
    /// active, striped). Shared between the non-scrollable and scrollable
    /// render paths.
    fn row_background(
        row_idx: usize,
        scroll_offset: usize,
        is_row_flash: bool,
        is_staged_for_delete: bool,
        is_active_row: bool,
        theme: &ThemePalette,
    ) -> Option<ratatui::style::Color> {
        if is_row_flash {
            Some(theme.component.feedback.yank_flash_bg)
        } else if is_staged_for_delete {
            Some(theme.component.table.staged_delete_bg)
        } else if is_active_row {
            Some(theme.component.table.result_row_active_bg)
        } else if (row_idx - scroll_offset) % 2 == 1 {
            Some(theme.component.table.striped_row_bg)
        } else {
            None
        }
    }

    /// Render the result table in Wrapped Cell Mode: all columns fit within
    /// `inner.width`, cell text wraps, and rows expand vertically.
    ///
    /// Returns the (unused) viewport plan so the caller can keep a consistent
    /// return shape; wrapped-cell never produces a horizontal scrollbar.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the normal render_table signature"
    )]
    fn render_wrapped_cell_table(
        frame: &mut Frame,
        inner: Rect,
        result: &QueryResult,
        ideal_widths: &[u16],
        settings: WrappedCellSettings,
        scroll_offset: usize,
        selection: &ResultSelection,
        editing_cell: Option<&EditingCellView<'_>>,
        staged_delete_rows: &BTreeSet<usize>,
        yank_flash: Option<YankFlash>,
        cell_vertical_offset: usize,
        now: Instant,
        stored_wrapped_cell: Option<&wrapped_cell_layout::MeasuredWrappedCellLayout>,
        key: wrapped_cell_layout::WrappedCellLayoutKey,
        theme: &ThemePalette,
    ) -> (
        ViewportPlan,
        usize,
        wrapped_cell_layout::MeasuredWrappedCellLayout,
    ) {
        let column_widths = wrapped_cell_layout::shrink_columns_to_fit(ideal_widths, inner.width);

        // Reuse previous frame's layout when key matches.
        let measured_layout = measured_wrapped_cell_layout(stored_wrapped_cell, key, || {
            result
                .rows
                .iter()
                .map(|row| {
                    wrapped_cell_layout::row_layout(
                        row,
                        &column_widths,
                        PADDING,
                        settings.max_lines_per_row,
                    )
                    .height
                })
                .collect()
        });
        let row_heights = &measured_layout.row_heights;

        let widths: Vec<Constraint> = column_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        let header = Row::new(result.columns.iter().map(|c| Cell::from(c.clone())))
            .style(
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .add_modifier(Modifier::BOLD)
                    .fg(theme.semantic.text.primary),
            )
            .height(1);

        let yank_flash_active = yank_flash.is_some_and(|f| now < f.until);
        let mut corrected_cell_vertical_offset = 0usize;
        let line_budget = inner.height.saturating_sub(RESULT_INNER_OVERHEAD) as usize;
        let mut visible: Vec<(usize, usize)> = Vec::new();
        let mut used = 0usize;
        let start = scroll_offset.min(row_heights.len());
        for (rel_idx, &h16) in row_heights[start..].iter().enumerate() {
            let abs_idx = start + rel_idx;
            let remaining = line_budget.saturating_sub(used);
            if remaining == 0 {
                break;
            }
            let h = (h16.max(1) as usize).min(remaining);
            visible.push((abs_idx, h));
            used += h;
        }

        let active_row = selection.row();
        let active_cell = selection.cell();
        let rows: Vec<Row> = visible
            .iter()
            .map(|&(abs_row_idx, height)| {
                let is_staged_for_delete = staged_delete_rows.contains(&abs_row_idx);
                let is_active_row = active_row == Some(abs_row_idx);
                let flash_scope = yank_flash
                    .filter(|f| yank_flash_active && f.row == abs_row_idx)
                    .map(|f| f.col);
                let is_row_flash = flash_scope == Some(None);
                let row_bg = Self::row_background(
                    abs_row_idx,
                    scroll_offset,
                    is_row_flash,
                    is_staged_for_delete,
                    is_active_row,
                    theme,
                );
                let row_data = &result.rows[abs_row_idx];
                let cells: Vec<Cell> = column_widths
                    .iter()
                    .enumerate()
                    .map(|(col_idx, &col_width)| {
                        let val = row_data.get(col_idx).map_or("", String::as_str);
                        let is_editing =
                            editing_cell.is_some_and(|e| e.row == abs_row_idx && e.col == col_idx);
                        let mut cell = if is_editing {
                            let e = editing_cell.expect("checked above");
                            render_editing_cell(e, col_width, theme)
                        } else {
                            let effective_max_lines = effective_row_line_cap(
                                settings.max_lines_per_row,
                                row_heights[abs_row_idx] as usize,
                                height,
                            );
                            let skip = if is_active_row && active_cell == Some(col_idx) {
                                let clamped = clamp_cell_vertical_offset(
                                    val,
                                    col_width,
                                    effective_max_lines,
                                    cell_vertical_offset,
                                );
                                corrected_cell_vertical_offset = clamped;
                                clamped
                            } else {
                                0
                            };
                            let lines =
                                wrapped_cell_lines(val, col_width, effective_max_lines, skip);
                            Cell::from(lines)
                        };

                        if !is_editing {
                            if is_row_flash || flash_scope == Some(Some(col_idx)) {
                                cell = cell.style(
                                    Style::default()
                                        .fg(theme.component.feedback.yank_flash_fg)
                                        .bg(theme.component.feedback.yank_flash_bg),
                                );
                            } else if is_staged_for_delete {
                                cell = cell.style(
                                    Style::default().fg(theme.component.table.staged_delete_fg),
                                );
                            } else if is_active_row && active_cell == Some(col_idx) {
                                cell = cell.style(
                                    Style::default()
                                        .bg(theme.component.table.result_cell_active_bg),
                                );
                            }
                        }
                        cell
                    })
                    .collect();

                let mut r = Row::new(cells).height(height.max(1) as u16);
                if let Some(bg) = row_bg {
                    r = r.style(Style::default().bg(bg));
                }
                r
            })
            .collect();

        let table = Table::new(rows, widths)
            .header(header)
            .style(Style::default().fg(theme.semantic.text.primary));
        frame.render_widget(table, inner);

        use crate::primitives::atoms::scroll_indicator::{
            VerticalScrollParams, render_vertical_scroll_indicator_bar,
        };
        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: visible.len(),
                total_items: result.rows.len(),
                has_horizontal_scrollbar: false,
            },
            theme,
        );

        (
            ViewportPlan::default(),
            corrected_cell_vertical_offset,
            measured_layout,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the normal render_table signature"
    )]
    fn render_wrapped_cell_table_scrollable(
        frame: &mut Frame,
        inner: Rect,
        result: &QueryResult,
        ideal_widths: &[u16],
        min_widths: &[u16],
        horizontal_offset: usize,
        settings: WrappedCellSettings,
        scroll_offset: usize,
        selection: &ResultSelection,
        editing_cell: Option<&EditingCellView<'_>>,
        staged_delete_rows: &BTreeSet<usize>,
        yank_flash: Option<YankFlash>,
        cell_vertical_offset: usize,
        now: Instant,
        stored_wrapped_cell: Option<&wrapped_cell_layout::MeasuredWrappedCellLayout>,
        key: wrapped_cell_layout::WrappedCellLayoutKey,
        theme: &ThemePalette,
    ) -> (
        ViewportPlan,
        usize,
        wrapped_cell_layout::MeasuredWrappedCellLayout,
    ) {
        let plan = ViewportPlan::calculate(ideal_widths, min_widths, inner.width);
        let clamped_offset = horizontal_offset.min(plan.max_offset);

        let config = ColumnWidthConfig {
            ideal_widths,
            min_widths,
        };
        let ctx = SelectionContext {
            horizontal_offset: clamped_offset,
            available_width: inner.width,
            fixed_count: Some(plan.column_count),
            max_offset: plan.max_offset,
        };
        let (viewport_indices, viewport_widths) = select_viewport_columns(&config, &ctx);

        if viewport_indices.is_empty() {
            return (
                plan,
                0,
                wrapped_cell_layout::MeasuredWrappedCellLayout::default(),
            );
        }

        // Reuse previous frame's layout when key matches. Include the visible
        // columns and widths because horizontal scrolling changes both.
        let viewport_key = key.with_viewport(&viewport_indices, &viewport_widths);
        let measured_layout =
            measured_wrapped_cell_layout(stored_wrapped_cell, viewport_key, || {
                result
                    .rows
                    .iter()
                    .map(|row| {
                        let viewport_row = project_viewport_row(row, &viewport_indices);
                        wrapped_cell_layout::row_layout(
                            &viewport_row,
                            &viewport_widths,
                            PADDING,
                            settings.max_lines_per_row,
                        )
                        .height
                    })
                    .collect()
            });
        let row_heights = &measured_layout.row_heights;

        let widths: Vec<Constraint> = viewport_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        let header = Row::new(viewport_indices.iter().map(|&idx| {
            let col_name = result.columns.get(idx).map_or("", String::as_str);
            Cell::from(col_name.to_string())
        }))
        .style(
            Style::default()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD)
                .fg(theme.semantic.text.primary),
        )
        .height(1);

        let active_row = selection.row();
        let active_cell = selection.cell();
        let yank_flash_active = yank_flash.is_some_and(|f| now < f.until);
        let mut corrected_cell_vertical_offset = 0usize;
        let line_budget = inner.height.saturating_sub(RESULT_INNER_OVERHEAD) as usize;
        let mut visible: Vec<(usize, usize)> = Vec::new();
        let mut used = 0usize;
        let start = scroll_offset.min(row_heights.len());
        for (rel_idx, &h16) in row_heights[start..].iter().enumerate() {
            let abs_idx = start + rel_idx;
            let remaining = line_budget.saturating_sub(used);
            if remaining == 0 {
                break;
            }
            let h = (h16.max(1) as usize).min(remaining);
            visible.push((abs_idx, h));
            used += h;
        }

        let rows: Vec<Row> = visible
            .iter()
            .map(|&(abs_row_idx, height)| {
                let is_staged_for_delete = staged_delete_rows.contains(&abs_row_idx);
                let is_active_row = active_row == Some(abs_row_idx);
                let flash_scope = yank_flash
                    .filter(|f| yank_flash_active && f.row == abs_row_idx)
                    .map(|f| f.col);
                let is_row_flash = flash_scope == Some(None);
                let row_bg = Self::row_background(
                    abs_row_idx,
                    scroll_offset,
                    is_row_flash,
                    is_staged_for_delete,
                    is_active_row,
                    theme,
                );

                let row_data = &result.rows[abs_row_idx];
                let cells: Vec<Cell> = viewport_indices
                    .iter()
                    .zip(viewport_widths.iter())
                    .map(|(&orig_idx, &col_width)| {
                        let val = row_data.get(orig_idx).map_or("", String::as_str);
                        let is_editing_cell =
                            editing_cell.is_some_and(|e| e.row == abs_row_idx && e.col == orig_idx);
                        let mut cell = if is_editing_cell {
                            let e = editing_cell.expect("checked above");
                            render_editing_cell(e, col_width, theme)
                        } else {
                            let effective_max_lines = effective_row_line_cap(
                                settings.max_lines_per_row,
                                row_heights[abs_row_idx] as usize,
                                height,
                            );
                            let skip = if is_active_row && active_cell == Some(orig_idx) {
                                let clamped = clamp_cell_vertical_offset(
                                    val,
                                    col_width,
                                    effective_max_lines,
                                    cell_vertical_offset,
                                );
                                corrected_cell_vertical_offset = clamped;
                                clamped
                            } else {
                                0
                            };
                            let lines =
                                wrapped_cell_lines(val, col_width, effective_max_lines, skip);
                            Cell::from(lines)
                        };

                        if !is_editing_cell {
                            if is_row_flash || flash_scope == Some(Some(orig_idx)) {
                                cell = cell.style(
                                    Style::default()
                                        .fg(theme.component.feedback.yank_flash_fg)
                                        .bg(theme.component.feedback.yank_flash_bg),
                                );
                            } else if is_staged_for_delete {
                                cell = cell.style(
                                    Style::default().fg(theme.component.table.staged_delete_fg),
                                );
                            } else if is_active_row && active_cell == Some(orig_idx) {
                                cell = cell.style(
                                    Style::default()
                                        .bg(theme.component.table.result_cell_active_bg),
                                );
                            }
                        }
                        cell
                    })
                    .collect();

                let mut r = Row::new(cells).height(height.max(1) as u16);
                if let Some(bg) = row_bg {
                    r = r.style(Style::default().bg(bg));
                }
                r
            })
            .collect();

        let table = Table::new(rows, widths)
            .header(header)
            .style(Style::default().fg(theme.semantic.text.primary));
        frame.render_widget(table, inner);

        let total_rows = result.rows.len();
        let total_cols = result.columns.len();

        use crate::primitives::atoms::scroll_indicator::{
            HorizontalScrollParams, VerticalScrollParams, render_horizontal_scroll_indicator,
            render_vertical_scroll_indicator_bar,
        };
        let has_h_scroll = plan.has_horizontal_scroll();
        render_vertical_scroll_indicator_bar(
            frame,
            inner,
            VerticalScrollParams {
                position: scroll_offset,
                viewport_size: visible.len(),
                total_items: total_rows,
                has_horizontal_scrollbar: has_h_scroll,
            },
            theme,
        );
        render_horizontal_scroll_indicator(
            frame,
            inner,
            HorizontalScrollParams {
                position: clamped_offset,
                viewport_size: plan.indicator_viewport_size(),
                total_items: total_cols,
                label: "col",
            },
            theme,
        );

        (plan, corrected_cell_vertical_offset, measured_layout)
    }
}

fn render_editing_cell(
    editing: &EditingCellView<'_>,
    col_width: u16,
    theme: &ThemePalette,
) -> Cell<'static> {
    let content = editing_cell_content(editing, col_width, theme);
    let foreground = if editing.actively_editing {
        theme.component.table.cell_edit_fg
    } else {
        theme.semantic.status.pending
    };

    Cell::from(content).style(
        Style::default()
            .bg(theme.component.table.result_cell_active_bg)
            .fg(foreground),
    )
}

fn editing_cell_content(
    editing: &EditingCellView<'_>,
    col_width: u16,
    theme: &ThemePalette,
) -> Line<'static> {
    if editing.actively_editing {
        cell_edit_line_with_cursor(editing.draft, editing.cursor, col_width as usize, theme)
    } else {
        Line::from(truncate_cell(editing.draft, col_width as usize))
    }
}

// TODO: cursor windowing is char-based; editing a CJK cell can render wider
// than the column until text_cursor_spans becomes display-width aware
fn cell_edit_line_with_cursor(
    text: &str,
    cursor: usize,
    max_chars: usize,
    theme: &ThemePalette,
) -> Line<'static> {
    let total = text.chars().count();

    if max_chars == 0 {
        return Line::from(vec![]);
    }

    let view_start = if cursor >= total {
        let effective = max_chars.saturating_sub(1);
        total.saturating_sub(effective)
    } else if cursor < max_chars {
        0
    } else {
        cursor.saturating_sub(max_chars / 2)
    };

    Line::from(text_cursor_spans(
        text, cursor, view_start, max_chars, theme,
    ))
}

fn truncate_cell(s: &str, max_width: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    truncate_to_width(first_line, max_width)
}

pub(crate) fn calculate_ideal_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<u16> {
    calculate_ideal_widths_inner(headers, rows, false)
}

/// Like `calculate_ideal_widths`, but sizes each column by the widest *line*
/// across the whole cell rather than just the first line. Wrapped Cell Mode
/// wraps and displays every line, so its width budgeting needs to reflect the
/// full multi-line content (e.g. `jsonb_pretty()` output, which often starts
/// with a lone `{` far shorter than the lines that follow).
pub(crate) fn calculate_wrapped_cell_ideal_widths(
    headers: &[String],
    rows: &[Vec<String>],
) -> Vec<u16> {
    calculate_ideal_widths_inner(headers, rows, true)
}

fn calculate_ideal_widths_inner(
    headers: &[String],
    rows: &[Vec<String>],
    consider_all_lines: bool,
) -> Vec<u16> {
    use unicode_width::UnicodeWidthStr;

    const SAMPLE_ROWS: usize = 50;

    headers
        .iter()
        .enumerate()
        .map(|(col_idx, header)| {
            let mut max_width = UnicodeWidthStr::width(header.as_str());

            let sample_size = rows.len().min(SAMPLE_ROWS);
            for row in rows.iter().take(sample_size) {
                if let Some(cell) = row.get(col_idx) {
                    if consider_all_lines {
                        for line in cell.lines() {
                            max_width = max_width.max(UnicodeWidthStr::width(line));
                        }
                    } else {
                        let first_line = cell.lines().next().unwrap_or(cell);
                        max_width = max_width.max(UnicodeWidthStr::width(first_line));
                    }
                }
            }

            let max_width = max_width.min(MAX_COL_WIDTH as usize) as u16;
            (max_width + PADDING).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
        })
        .collect()
}

/// The runtime-effective Wrapped Cell settings for the result pane: the
/// persisted config only applies when the runtime toggle (`Alt+L`) is on; when off,
/// horizontal scrolling is always allowed so the normal viewport path runs.
/// Reuse the previous frame's measured Wrapped Cell layout when its key still
/// matches, otherwise measure fresh via `compute`.
///
/// Measuring per-row heights means wrapping every cell of every row — O(rows ×
/// cols) unicode-width work, the expensive part of a Wrapped Cell draw for a
/// large result. The key only changes on a new result, a resize, or a setting
/// toggle, so plain scrolling hits the reuse path, which just clones two flat
/// integer vectors (a cheap memcpy) instead of re-wrapping every row.
fn measured_wrapped_cell_layout(
    stored: Option<&wrapped_cell_layout::MeasuredWrappedCellLayout>,
    key: wrapped_cell_layout::WrappedCellLayoutKey,
    compute: impl FnOnce() -> Vec<u16>,
) -> wrapped_cell_layout::MeasuredWrappedCellLayout {
    if let Some(stored) = stored
        && stored.key == key
    {
        return stored.clone();
    }
    wrapped_cell_layout::MeasuredWrappedCellLayout::new(compute(), key)
}

fn cached_wrapped_cell_ideal_widths<'a>(
    cache: &'a mut ColumnWidthsCache,
    headers: &[String],
    rows: &[Vec<String>],
) -> &'a [u16] {
    if cache.wrapped_ideal_widths.is_none() {
        cache.wrapped_ideal_widths = Some(calculate_wrapped_cell_ideal_widths(headers, rows));
    }
    cache
        .wrapped_ideal_widths
        .as_deref()
        .expect("wrapped ideal widths are initialized above")
}

fn project_viewport_row(row: &[String], indices: &[usize]) -> Vec<String> {
    indices
        .iter()
        .map(|&index| row.get(index).cloned().unwrap_or_default())
        .collect()
}

fn needs_wrapped_cell_horizontal_scroll(column_count: usize, available_width: u16) -> bool {
    let minimum_total = column_count.saturating_mul(2).saturating_sub(1);
    usize::from(available_width) < minimum_total
}

/// The line cap actually in effect for a row: the configured `max_lines_per_row`
/// tightened by whatever the pane can physically display this frame.
///
/// The viewport row-selection loop fills the pane top to bottom and clips
/// whichever row lands last to however much space remains below it, rather
/// than dropping it entirely — that keeps the pane fully painted instead of
/// leaving a gap. `row_height` is the row's full desired height and
/// `line_budget` here is that row's actual on-screen allowance (not
/// necessarily the whole pane). When the row got clipped we also treat that
/// allowance as a cap here, the same way a configured `max_lines_per_row` is:
/// this is what makes the ellipsis appear and Ctrl+J/K scrolling available in
/// that case, not just when a row cap is configured.
fn effective_row_line_cap(
    settings_cap: Option<u16>,
    row_height: usize,
    line_budget: usize,
) -> Option<u16> {
    if line_budget == 0 || row_height <= line_budget {
        return settings_cap;
    }
    let viewport_cap = line_budget.min(u16::MAX as usize) as u16;
    Some(settings_cap.map_or(viewport_cap, |cap| cap.min(viewport_cap)))
}

/// The number of additional wrapped lines available below `cell_vertical_offset`
/// for the active cell, clamped to what the cell's content actually has (0 when
/// the cap doesn't truncate it, since there is nothing to scroll into).
fn clamp_cell_vertical_offset(
    text: &str,
    col_width: u16,
    max_lines_per_row: Option<u16>,
    offset: usize,
) -> usize {
    let Some(cap) = max_lines_per_row else {
        return 0;
    };
    let total = WrappedCellSettings::wrapped_cell_lines(text, col_width, PADDING) as usize;
    offset.min(total.saturating_sub(cap as usize))
}

/// Wrap a cell's text into ratatui lines for Wrapped Cell Mode, applying the
/// optional row cap with a trailing "..." on the last line when truncated.
/// `skip` drops that many wrapped lines from the top first (scrolling into a
/// truncated cell).
fn wrapped_cell_lines(
    text: &str,
    col_width: u16,
    max_lines: Option<u16>,
    skip: usize,
) -> Vec<Line<'static>> {
    let wrap_width = WrappedCellSettings::effective_wrap_width(col_width, PADDING);
    let mut lines = crate::primitives::utils::text_utils::wrap_text_lines(text, wrap_width);

    if skip > 0 {
        lines.drain(..skip.min(lines.len()));
    }

    if let Some(cap) = max_lines {
        let cap = cap as usize;
        if lines.len() > cap {
            lines.truncate(cap);
            if let Some(last) = lines.last_mut() {
                append_ellipsis(last, wrap_width);
            }
        }
    }

    lines.into_iter().map(Line::from).collect()
}

/// Append "..." to `line`, trimming it first if needed so the result never
/// exceeds `width` display cells. When `width` is too small for the ellipsis
/// itself, the line is truncated to fit as many dots as possible.
fn append_ellipsis(line: &mut String, width: u16) {
    let ellipsis = "...";
    let max = width as usize;
    let current = unicode_width::UnicodeWidthStr::width(line.as_str());
    let ellipsis_width = unicode_width::UnicodeWidthStr::width(ellipsis);
    if current + ellipsis_width <= max {
        line.push_str(ellipsis);
        return;
    }
    if max == 0 {
        *line = String::new();
        return;
    }
    if ellipsis_width > max {
        *line = crate::primitives::utils::text_utils::take_within_width(ellipsis, max);
        return;
    }
    let budget = max.saturating_sub(ellipsis_width);
    let trimmed = crate::primitives::utils::text_utils::take_within_width(line, budget);
    *line = trimmed;
    line.push_str(ellipsis);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    use crate::theme::DEFAULT_THEME;

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
            assert_eq!(result[0], 4);
            assert_eq!(result[1], 7);
        }

        #[test]
        fn respects_max_width_constraint() {
            let headers = vec!["description".to_string()];
            let long_text = "a".repeat(300);
            let rows = vec![vec![long_text]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            assert_eq!(result[0], 200);
        }

        #[test]
        fn handles_multibyte_characters_correctly() {
            let headers = vec!["名前".to_string()];
            let rows = vec![vec!["日本語テスト".to_string()]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
            assert_eq!(result[0], 14);
        }

        #[test]
        fn only_considers_first_line_for_multiline_cells() {
            let headers = vec!["text".to_string()];
            let rows = vec![vec![
                "short\nvery long second line that should be ignored".to_string(),
            ]];

            let result = calculate_ideal_widths(&headers, &rows);

            assert_eq!(result.len(), 1);
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
            assert_eq!(result[0], 4);
            assert_eq!(result[1], 15);
            assert_eq!(result[2], 19);
        }
    }

    mod calculate_wrapped_cell_ideal_widths_tests {
        use super::*;

        #[test]
        fn considers_widest_line_not_just_first() {
            let headers = vec!["data".to_string()];
            let rows = vec![vec![
                "{\n    \"name\": \"a very long value here\"\n}".to_string(),
            ]];

            let ideal = calculate_ideal_widths(&headers, &rows);
            let wrapped_cell = calculate_wrapped_cell_ideal_widths(&headers, &rows);

            assert!(wrapped_cell[0] > ideal[0]);
            assert_eq!(wrapped_cell[0], 38);
        }

        #[test]
        fn cached_widths_are_reused_for_the_same_result() {
            let headers = vec!["value".to_string()];
            let rows = vec![vec!["short".to_string()]];
            let changed_rows = vec![vec!["a much longer value".to_string()]];
            let mut cache = ColumnWidthsCache::new(vec![7], vec![7], 1);

            let first = cached_wrapped_cell_ideal_widths(&mut cache, &headers, &rows).to_vec();
            let reused = cached_wrapped_cell_ideal_widths(&mut cache, &headers, &changed_rows);

            assert_eq!(reused, first.as_slice());
        }
    }

    #[test]
    fn scrollable_measurement_projects_visible_columns() {
        let row = vec!["a".to_string(), "x".to_string(), "1234567890".to_string()];
        let indices = [2, 0];
        let widths = [4, 10];
        let projected = project_viewport_row(&row, &indices);

        let projected_height =
            wrapped_cell_layout::row_layout(&projected, &widths, PADDING, None).height;
        let unprojected_height =
            wrapped_cell_layout::row_layout(&row, &widths, PADDING, None).height;

        assert_eq!(projected_height, 5);
        assert_eq!(unprojected_height, 1);
    }

    #[test]
    fn viewport_changes_invalidate_measured_layout() {
        let key = wrapped_cell_layout::WrappedCellLayoutKey::default();
        let first_key = key.with_viewport(&[0], &[4]);
        let first = measured_wrapped_cell_layout(None, first_key, || vec![1]);
        let second_key = key.with_viewport(&[1], &[4]);
        let second = measured_wrapped_cell_layout(Some(&first), second_key, || vec![5]);

        assert_eq!(second.row_heights, vec![5]);
    }

    #[test]
    fn extremely_narrow_wrapped_cell_uses_horizontal_scroll() {
        assert!(needs_wrapped_cell_horizontal_scroll(4, 6));
        assert!(!needs_wrapped_cell_horizontal_scroll(4, 7));
    }

    #[test]
    fn actively_editing_cell_content_marks_cursor() {
        let editing = EditingCellView {
            row: 0,
            col: 0,
            draft: "abc",
            actively_editing: true,
            cursor: 1,
        };

        let line = editing_cell_content(&editing, 10, &DEFAULT_THEME);
        let spans: Vec<String> = line
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect();

        assert_eq!(spans, ["a", "b", "c"]);
        assert_eq!(line.spans[1].style, DEFAULT_THEME.block_cursor_style());
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
    fn multibyte_truncates_by_display_width() {
        let result = truncate_cell("こんにちは世界", 5);

        assert_eq!(result, "こ...");
    }

    #[rstest]
    #[case("日本語テスト", 12, "日本語テスト")]
    #[case("日本語テスト", 10, "日本語...")]
    #[case("日本語テスト", 5, "日...")]
    #[case("日本語テスト", 4, "...")]
    #[case("SELECT * FROM 日本語テーブル", 15, "SELECT * FRO...")]
    fn multibyte_truncation_is_safe(
        #[case] input: &str,
        #[case] max: usize,
        #[case] expected: &str,
    ) {
        use unicode_width::UnicodeWidthStr;

        let result = truncate_cell(input, max);

        assert_eq!(result, expected);
        assert!(UnicodeWidthStr::width(result.as_str()) <= max);
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
    fn zero_width_returns_empty() {
        let result = truncate_cell("hello", 0);

        assert_eq!(result, "");
    }

    #[rstest]
    #[case(1, ".")]
    #[case(2, "..")]
    #[case(3, "...")]
    #[case(4, "h...")]
    #[case(5, "he...")]
    fn small_widths_stay_within_contract(#[case] max: usize, #[case] expected: &str) {
        let result = truncate_cell("hello world", max);

        assert_eq!(result, expected);
    }

    #[test]
    #[ignore = "local-only dev benchmark, not tied to a CI issue"]
    #[allow(clippy::print_stderr, reason = "benchmark result output")]
    fn bench_ideal_widths_cache_speedup() {
        use crate::app::model::shared::viewport::ColumnWidthsCache;
        use crate::primitives::utils::text_utils::calculate_header_min_widths;
        use std::time::Instant;

        let cols = 20;
        let rows = 50;
        let headers: Vec<String> = (0..cols).map(|i| format!("column_{i}")).collect();
        let data: Vec<Vec<String>> = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| format!("value_r{r}_c{c}_padding"))
                    .collect()
            })
            .collect();

        let iterations = 1000;

        // Baseline: compute both widths every iteration (pre-optimization path)
        let start = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(calculate_ideal_widths(&headers, &data));
            std::hint::black_box(calculate_header_min_widths(&headers));
        }
        let baseline = start.elapsed();

        // Cached: is_valid check + clone (actual cache-hit path)
        let ideal = calculate_ideal_widths(&headers, &data);
        let min = calculate_header_min_widths(&headers);
        let cache = ColumnWidthsCache::new(ideal, min, 1);
        let start = Instant::now();
        for _ in 0..iterations {
            let valid = std::hint::black_box(cache.is_valid(1));
            if valid {
                std::hint::black_box(cache.clone());
            }
        }
        let cached = start.elapsed();

        eprintln!(
            "Baseline: {:?} ({:.1} µs/iter), Cached (is_valid+clone): {:?} ({:.1} µs/iter), Speedup: {:.0}x",
            baseline,
            baseline.as_micros() as f64 / iterations as f64,
            cached,
            cached.as_micros() as f64 / iterations as f64,
            baseline.as_secs_f64() / cached.as_secs_f64(),
        );
    }

    mod cell_vertical_scroll {
        use super::*;

        #[rstest]
        #[case(0, "")]
        #[case(1, ".")]
        #[case(2, "..")]
        fn append_ellipsis_stays_within_narrow_width(#[case] width: u16, #[case] expected: &str) {
            let mut line = String::from("long");

            append_ellipsis(&mut line, width);

            assert_eq!(line, expected);
        }

        #[test]
        fn wrapped_cell_lines_skip_drops_leading_lines() {
            let lines = wrapped_cell_lines("a\nb\nc\nd", 10, None, 2);

            assert_eq!(lines.len(), 2);
        }

        #[test]
        fn wrapped_cell_lines_skip_beyond_len_returns_empty() {
            let lines = wrapped_cell_lines("a\nb", 10, None, 10);

            assert!(lines.is_empty());
        }

        #[test]
        fn wrapped_cell_lines_skip_still_caps_and_ellipsizes() {
            let lines = wrapped_cell_lines("a\nb\nc\nd\ne", 10, Some(2), 1);

            assert_eq!(lines.len(), 2);
        }

        #[test]
        fn clamp_offset_is_zero_when_no_cap() {
            let clamped = clamp_cell_vertical_offset("a\nb\nc\nd", 10, None, 5);

            assert_eq!(clamped, 0);
        }

        #[test]
        fn clamp_offset_is_zero_when_cell_not_truncated() {
            let clamped = clamp_cell_vertical_offset("a\nb", 10, Some(5), 5);

            assert_eq!(clamped, 0);
        }

        #[test]
        fn clamp_offset_caps_at_overflow_amount() {
            // 5 wrapped lines, cap 2 -> 3 lines of overflow to scroll into.
            let clamped = clamp_cell_vertical_offset("a\nb\nc\nd\ne", 10, Some(2), 100);

            assert_eq!(clamped, 3);
        }

        #[test]
        fn clamp_offset_passes_through_when_within_range() {
            let clamped = clamp_cell_vertical_offset("a\nb\nc\nd\ne", 10, Some(2), 1);

            assert_eq!(clamped, 1);
        }
    }

    mod effective_row_line_cap_tests {
        use super::*;

        #[test]
        fn no_cap_and_row_fits_stays_uncapped() {
            assert_eq!(effective_row_line_cap(None, 5, 20), None);
        }

        #[test]
        fn no_configured_cap_but_row_overflows_viewport_gets_capped_to_budget() {
            // No max_lines_per_row set, but the row's natural height (50) is taller
            // than what the pane can show (20 lines) -- the viewport itself must
            // now act as the cap so an ellipsis appears and Ctrl+J/K can scroll.
            assert_eq!(effective_row_line_cap(None, 50, 20), Some(20));
        }

        #[test]
        fn configured_cap_tighter_than_viewport_is_unchanged() {
            assert_eq!(effective_row_line_cap(Some(5), 5, 20), Some(5));
        }

        #[test]
        fn configured_cap_looser_than_viewport_is_tightened_to_viewport() {
            // cap=50 lets the row grow to 50 lines, but only 20 fit on screen.
            assert_eq!(effective_row_line_cap(Some(50), 50, 20), Some(20));
        }

        #[test]
        fn zero_line_budget_leaves_settings_cap_unchanged() {
            assert_eq!(effective_row_line_cap(Some(5), 5, 0), Some(5));
            assert_eq!(effective_row_line_cap(None, 5, 0), None);
        }
    }
}
