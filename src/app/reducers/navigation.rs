//! Navigation sub-reducer: focus, scroll, selection, filter, command line.

use std::time::{Duration, Instant};

use crate::app::action::{Action, ConnectionsLoadedPayload};
use crate::app::effect::Effect;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::palette::palette_command_count;
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::app::ui_state::YankFlash;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};
use crate::app::write_guardrails::{
    TargetSummary, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::write_update::build_pk_pairs;

use super::helpers::{
    ERR_DELETION_REQUIRES_PRIMARY_KEY, ERR_EDITING_REQUIRES_PRIMARY_KEY,
    deletion_refresh_target_bulk, editable_preview_base,
};

fn result_row_count(state: &AppState) -> usize {
    state
        .query
        .current_result
        .as_ref()
        .map(|r| r.rows.len())
        .unwrap_or(0)
}

fn result_col_count(state: &AppState) -> usize {
    state
        .query
        .current_result
        .as_ref()
        .map(|r| r.columns.len())
        .unwrap_or(0)
}

fn result_max_scroll(state: &AppState) -> usize {
    let visible = state.result_visible_rows();
    result_row_count(state).saturating_sub(visible)
}

/// Adjust viewport scroll so the active row stays visible.
fn ensure_row_visible(state: &mut AppState) {
    if let Some(row) = state.ui.result_selection.row() {
        let visible = state.result_visible_rows();
        if visible == 0 {
            return;
        }
        if row < state.ui.result_scroll_offset {
            state.ui.result_scroll_offset = row;
        } else if row >= state.ui.result_scroll_offset + visible {
            state.ui.result_scroll_offset = row - visible + 1;
        }
    }
}

/// Move row cursor to `new_row` if row is active, otherwise apply `scroll_fn` to viewport offset.
fn move_row_or_scroll(state: &mut AppState, new_row: usize, scroll_fn: impl FnOnce(&mut AppState)) {
    if state.ui.result_selection.row().is_some() {
        state.ui.result_selection.move_row(new_row);
        ensure_row_visible(state);
    } else {
        scroll_fn(state);
    }
}

/// Adjust horizontal offset so the active cell stays visible.
fn ensure_cell_visible(state: &mut AppState) {
    if let Some(col) = state.ui.result_selection.cell() {
        let plan = &state.ui.result_viewport_plan;
        let h_offset = state.ui.result_horizontal_offset;
        if col < h_offset {
            state.ui.result_horizontal_offset = col;
        } else if col >= h_offset + plan.column_count {
            state.ui.result_horizontal_offset =
                col.saturating_sub(plan.column_count.saturating_sub(1));
        }
    }
}

fn inspector_total_items(state: &AppState, services: &AppServices) -> usize {
    state
        .cache
        .table_detail
        .as_ref()
        .map(|t| match state.ui.inspector_tab {
            InspectorTab::Info => 5,
            InspectorTab::Columns => t.columns.len(),
            InspectorTab::Indexes => t.indexes.len(),
            InspectorTab::ForeignKeys => t.foreign_keys.len(),
            InspectorTab::Rls => t.rls.as_ref().map_or(1, |rls| {
                let mut lines = 1;
                if !rls.policies.is_empty() {
                    lines += 2;
                    for policy in &rls.policies {
                        lines += 1;
                        if policy.qual.is_some() {
                            lines += 1;
                        }
                    }
                }
                lines
            }),
            InspectorTab::Triggers => t.triggers.len(),
            InspectorTab::Ddl => services.ddl_generator.ddl_line_count(t),
        })
        .unwrap_or(0)
}

fn inspector_max_scroll(state: &AppState, services: &AppServices) -> usize {
    let visible = match state.ui.inspector_tab {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };
    inspector_total_items(state, services).saturating_sub(visible)
}

fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}

/// Builds a WritePreview for bulk-deleting all staged rows via a single DELETE statement.
pub fn build_bulk_delete_preview(
    state: &AppState,
    services: &AppServices,
) -> Result<(WritePreview, usize, Option<usize>), String> {
    if state.ui.staged_delete_rows.is_empty() {
        return Err("No rows staged for deletion".to_string());
    }
    if state.runtime.dsn.is_none() {
        return Err("No active connection".to_string());
    }
    if state.query.status != crate::app::query_execution::QueryStatus::Idle {
        return Err("Write is unavailable while query is running".to_string());
    }

    let (result, pk_cols) = editable_preview_base(state).map_err(|msg| {
        if msg == ERR_EDITING_REQUIRES_PRIMARY_KEY {
            ERR_DELETION_REQUIRES_PRIMARY_KEY.to_string()
        } else {
            msg
        }
    })?;

    let mut pk_pairs_per_row: Vec<Vec<(String, String)>> = Vec::new();
    for &row_idx in &state.ui.staged_delete_rows {
        let row = result
            .rows
            .get(row_idx)
            .ok_or_else(|| format!("Staged row index {} out of bounds", row_idx))?;
        let pairs = build_pk_pairs(&result.columns, row, pk_cols)
            .ok_or_else(|| "Stable key columns are not present in current result".to_string())?;
        pk_pairs_per_row.push(pairs);
    }

    let sql = services.sql_dialect.build_bulk_delete_sql(
        &state.query.pagination.schema,
        &state.query.pagination.table,
        &pk_pairs_per_row,
    );

    let staged_count = state.ui.staged_delete_rows.len();
    let first_deleted_idx = *state.ui.staged_delete_rows.iter().next().unwrap();
    let (target_page, target_row) = deletion_refresh_target_bulk(
        result.rows.len(),
        staged_count,
        first_deleted_idx,
        state.query.pagination.current_page,
    );

    let target = TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: pk_pairs_per_row.first().cloned().unwrap_or_default(),
    };
    let guardrail = evaluate_guardrails(true, true, Some(target.clone()));

    Ok((
        WritePreview {
            operation: WriteOperation::Delete,
            sql,
            target_summary: target,
            diff: vec![],
            guardrail,
        },
        target_page,
        target_row,
    ))
}

fn editable_cell_context(state: &AppState) -> Result<(usize, usize, String), String> {
    let row_idx = state
        .ui
        .result_selection
        .row()
        .ok_or_else(|| "No active row".to_string())?;
    let col_idx = state
        .ui
        .result_selection
        .cell()
        .ok_or_else(|| "No active cell".to_string())?;

    let (result, pk_cols) = editable_preview_base(state)?;

    let column_name = result
        .columns
        .get(col_idx)
        .ok_or_else(|| "Column index out of bounds".to_string())?;
    if pk_cols.iter().any(|pk| pk == column_name) {
        return Err("Primary key columns are read-only".to_string());
    }

    let row = result
        .rows
        .get(row_idx)
        .ok_or_else(|| "Row index out of bounds".to_string())?;
    if build_pk_pairs(&result.columns, row, pk_cols).is_none() {
        return Err("Stable key columns are not present in current result".to_string());
    }

    let cell_value = row
        .get(col_idx)
        .ok_or_else(|| "Cell index out of bounds".to_string())?
        .clone();

    Ok((row_idx, col_idx, cell_value))
}

/// Handles focus, scroll, selection, filter, and command line actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_navigation(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        Action::SetFocusedPane(pane) => {
            if *pane != FocusedPane::Result {
                state.ui.result_selection.reset();
                state.cell_edit.clear();
                state.ui.staged_delete_rows.clear();
                state.pending_write_preview = None;
                if state.ui.input_mode == InputMode::CellEdit {
                    state.ui.input_mode = InputMode::Normal;
                }
            }
            state.ui.focused_pane = *pane;
            Some(vec![])
        }
        Action::ToggleFocus => {
            let was_focus = state.ui.focus_mode;
            state.toggle_focus();
            if was_focus {
                state.ui.result_selection.reset();
                state.cell_edit.clear();
                state.ui.staged_delete_rows.clear();
                state.pending_write_preview = None;
            }
            Some(vec![])
        }
        Action::InspectorNextTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.next();
            Some(vec![])
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.prev();
            Some(vec![])
        }

        // Clipboard paste
        Action::Paste(text) => match state.ui.input_mode {
            InputMode::TablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.filter_input.push_str(&clean);
                state.ui.reset_picker_selection();
                Some(vec![])
            }
            InputMode::ErTablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.er_filter_input.push_str(&clean);
                state.ui.reset_er_picker_selection();
                Some(vec![])
            }
            InputMode::CommandLine => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.command_line_input.push_str(&clean);
                Some(vec![])
            }
            InputMode::CellEdit => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.cell_edit.input.insert_str(&clean);
                Some(vec![])
            }
            _ => None,
        },

        // Filter
        Action::FilterInput(c) => {
            state.ui.filter_input.push(*c);
            state.ui.reset_picker_selection();
            Some(vec![])
        }
        Action::FilterBackspace => {
            state.ui.filter_input.pop();
            state.ui.reset_picker_selection();
            Some(vec![])
        }

        // Command Line
        Action::EnterCommandLine => {
            state.ui.command_line_return_mode = state.ui.input_mode;
            state.ui.input_mode = InputMode::CommandLine;
            state.command_line_input.clear();
            Some(vec![])
        }
        Action::ExitCommandLine => {
            state.ui.input_mode = state.ui.command_line_return_mode;
            state.ui.command_line_return_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::CommandLineInput(c) => {
            state.command_line_input.push(*c);
            Some(vec![])
        }
        Action::CommandLineBackspace => {
            state.command_line_input.pop();
            Some(vec![])
        }

        // Selection
        Action::SelectNext => {
            match state.ui.input_mode {
                InputMode::TablePicker => {
                    let max = state.filtered_tables().len().saturating_sub(1);
                    if state.ui.picker_selected < max {
                        state.ui.set_picker_selection(state.ui.picker_selected + 1);
                    }
                }
                InputMode::ErTablePicker => {
                    let max = state.er_filtered_tables().len().saturating_sub(1);
                    if state.ui.er_picker_selected < max {
                        state
                            .ui
                            .set_er_picker_selection(state.ui.er_picker_selected + 1);
                    }
                }
                InputMode::CommandPalette => {
                    let max = palette_command_count() - 1;
                    if state.ui.picker_selected < max {
                        state.ui.set_picker_selection(state.ui.picker_selected + 1);
                    }
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer {
                        let len = state.tables().len();
                        if len > 0 && state.ui.explorer_selected < len - 1 {
                            state
                                .ui
                                .set_explorer_selection(Some(state.ui.explorer_selected + 1));
                        }
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectPrevious => {
            match state.ui.input_mode {
                InputMode::TablePicker | InputMode::CommandPalette => {
                    state
                        .ui
                        .set_picker_selection(state.ui.picker_selected.saturating_sub(1));
                }
                InputMode::ErTablePicker => {
                    state
                        .ui
                        .set_er_picker_selection(state.ui.er_picker_selected.saturating_sub(1));
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty()
                    {
                        let new_idx = state.ui.explorer_selected.saturating_sub(1);
                        state.ui.set_explorer_selection(Some(new_idx));
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectFirst => {
            match state.ui.input_mode {
                InputMode::TablePicker | InputMode::CommandPalette => {
                    state.ui.reset_picker_selection();
                }
                InputMode::ErTablePicker => {
                    state.ui.reset_er_picker_selection();
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty()
                    {
                        state.ui.set_explorer_selection(Some(0));
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectLast => {
            match state.ui.input_mode {
                InputMode::TablePicker => {
                    let max = state.filtered_tables().len().saturating_sub(1);
                    state.ui.set_picker_selection(max);
                }
                InputMode::ErTablePicker => {
                    let max = state.er_filtered_tables().len().saturating_sub(1);
                    state.ui.set_er_picker_selection(max);
                }
                InputMode::CommandPalette => {
                    state.ui.set_picker_selection(palette_command_count() - 1);
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer {
                        let len = state.tables().len();
                        if len > 0 {
                            state.ui.set_explorer_selection(Some(len - 1));
                        }
                    }
                }
                _ => {}
            }
            Some(vec![])
        }

        // Explorer page scroll (selection-based)
        Action::SelectHalfPageDown => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = (visible / 2).max(1);
            let max_idx = len.saturating_sub(1);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectHalfPageUp => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = (visible / 2).max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectFullPageDown => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = visible.max(1);
            let max_idx = len.saturating_sub(1);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectFullPageUp => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = visible.max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }

        // Result Scroll (when row is active, these move the row cursor instead)
        Action::ResultScrollUp => {
            let new_row = state
                .ui
                .result_selection
                .row()
                .and_then(|r| r.checked_sub(1));
            match new_row {
                Some(r) => move_row_or_scroll(state, r, |_| {}),
                None if state.ui.result_selection.row().is_none() => {
                    state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
                }
                _ => {} // row == 0, no-op
            }
            Some(vec![])
        }
        Action::ResultScrollDown => {
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + 1).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max_scroll = result_max_scroll(s);
                if s.ui.result_scroll_offset < max_scroll {
                    s.ui.result_scroll_offset += 1;
                }
            });
            Some(vec![])
        }
        Action::ResultScrollTop => {
            move_row_or_scroll(state, 0, |s| s.ui.result_scroll_offset = 0);
            Some(vec![])
        }
        Action::ResultScrollBottom => {
            let max_row = result_row_count(state).saturating_sub(1);
            let max_scroll = result_max_scroll(state);
            move_row_or_scroll(state, max_row, |s| s.ui.result_scroll_offset = max_scroll);
            Some(vec![])
        }
        Action::ResultScrollHalfPageDown => {
            let delta = (state.result_visible_rows() / 2).max(1);
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + delta).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max = result_max_scroll(s);
                s.ui.result_scroll_offset = (s.ui.result_scroll_offset + delta).min(max);
            });
            Some(vec![])
        }
        Action::ResultScrollHalfPageUp => {
            let delta = (state.result_visible_rows() / 2).max(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| r.saturating_sub(delta))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                s.ui.result_scroll_offset = s.ui.result_scroll_offset.saturating_sub(delta);
            });
            Some(vec![])
        }
        Action::ResultScrollFullPageDown => {
            let delta = state.result_visible_rows().max(1);
            let max_row = result_row_count(state).saturating_sub(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| (r + delta).min(max_row))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                let max = result_max_scroll(s);
                s.ui.result_scroll_offset = (s.ui.result_scroll_offset + delta).min(max);
            });
            Some(vec![])
        }
        Action::ResultScrollFullPageUp => {
            let delta = state.result_visible_rows().max(1);
            let new_row = state
                .ui
                .result_selection
                .row()
                .map(|r| r.saturating_sub(delta))
                .unwrap_or(0);
            move_row_or_scroll(state, new_row, |s| {
                s.ui.result_scroll_offset = s.ui.result_scroll_offset.saturating_sub(delta);
            });
            Some(vec![])
        }
        Action::ResultScrollLeft => {
            state.ui.result_horizontal_offset =
                calculate_prev_column_offset(state.ui.result_horizontal_offset);
            Some(vec![])
        }
        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.result_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.result_horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }

        // Inspector Scroll
        Action::InspectorScrollUp => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::InspectorScrollDown => {
            let max_offset = inspector_max_scroll(state, services);
            if state.ui.inspector_scroll_offset < max_offset {
                state.ui.inspector_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::InspectorScrollTop => {
            state.ui.inspector_scroll_offset = 0;
            Some(vec![])
        }
        Action::InspectorScrollBottom => {
            state.ui.inspector_scroll_offset = inspector_max_scroll(state, services);
            Some(vec![])
        }
        Action::InspectorScrollHalfPageDown => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = (visible / 2).max(1);
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset = (state.ui.inspector_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::InspectorScrollHalfPageUp => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = (visible / 2).max(1);
            state.ui.inspector_scroll_offset =
                state.ui.inspector_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::InspectorScrollFullPageDown => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = visible.max(1);
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset = (state.ui.inspector_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::InspectorScrollFullPageUp => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = visible.max(1);
            state.ui.inspector_scroll_offset =
                state.ui.inspector_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::InspectorScrollLeft => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
            Some(vec![])
        }
        Action::InspectorScrollRight => {
            let plan = &state.ui.inspector_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.inspector_horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }

        // Explorer Scroll
        Action::ExplorerScrollLeft => {
            state.ui.explorer_horizontal_offset =
                state.ui.explorer_horizontal_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::ExplorerScrollRight => {
            let max_name_width = state
                .tables()
                .iter()
                .map(|t| t.qualified_name().len())
                .max()
                .unwrap_or(0);
            if state.ui.explorer_horizontal_offset < max_name_width {
                state.ui.explorer_horizontal_offset += 1;
            }
            Some(vec![])
        }

        // Result pane selection
        Action::ResultEnterRowActive => {
            let rows = result_row_count(state);
            if rows > 0 {
                let clamped = state.ui.result_scroll_offset.min(rows - 1);
                state.ui.result_selection.enter_row(clamped);
            }
            Some(vec![])
        }
        Action::ResultEnterCellActive => {
            if state.ui.result_selection.row().is_some() {
                state
                    .ui
                    .result_selection
                    .enter_cell(state.ui.result_horizontal_offset);
            }
            Some(vec![])
        }
        Action::ResultExitToRowActive => {
            state.ui.result_selection.exit_to_row();
            state.cell_edit.clear();
            state.pending_write_preview = None;
            Some(vec![])
        }
        Action::ResultExitToScroll => {
            state.ui.result_selection.reset();
            state.cell_edit.clear();
            state.ui.staged_delete_rows.clear();
            state.pending_write_preview = None;
            Some(vec![])
        }
        Action::ResultCellLeft => {
            if let Some(c) = state.ui.result_selection.cell()
                && c > 0
            {
                state.ui.result_selection.enter_cell(c - 1);
                ensure_cell_visible(state);
            }
            Some(vec![])
        }
        Action::ResultCellRight => {
            if let Some(c) = state.ui.result_selection.cell() {
                let max_col = result_col_count(state).saturating_sub(1);
                if c < max_col {
                    state.ui.result_selection.enter_cell(c + 1);
                    ensure_cell_visible(state);
                }
            }
            Some(vec![])
        }
        Action::ResultCellYank => {
            if let (Some(row_idx), Some(col_idx)) = (
                state.ui.result_selection.row(),
                state.ui.result_selection.cell(),
            ) {
                let content = state
                    .query
                    .current_result
                    .as_ref()
                    .and_then(|r| r.rows.get(row_idx))
                    .and_then(|row| row.get(col_idx))
                    .cloned();
                match content {
                    Some(value) => {
                        state.ui.yank_flash = Some(YankFlash {
                            row: row_idx,
                            col: Some(col_idx),
                            until: now + Duration::from_millis(200),
                        });
                        Some(vec![Effect::CopyToClipboard {
                            content: value,
                            on_success: Some(Action::CellCopied),
                            on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                        }])
                    }
                    None => {
                        state
                            .messages
                            .set_error_at("Cell index out of bounds".into(), now);
                        Some(vec![])
                    }
                }
            } else {
                Some(vec![])
            }
        }
        Action::DdlYank => {
            if state.ui.inspector_tab == InspectorTab::Ddl
                && let Some(table) = state.cache.table_detail.as_ref()
            {
                let ddl = services.ddl_generator.generate_ddl(table);
                return Some(vec![Effect::CopyToClipboard {
                    content: ddl,
                    on_success: Some(Action::CellCopied),
                    on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                }]);
            }
            Some(vec![])
        }
        Action::ResultRowYankOperatorPending => {
            state.ui.yank_op_pending = true;
            Some(vec![])
        }
        Action::ResultRowYank => {
            if let Some(row_idx) = state.ui.result_selection.row() {
                let content = state
                    .query
                    .current_result
                    .as_ref()
                    .and_then(|r| r.rows.get(row_idx))
                    .map(|row| {
                        row.iter()
                            .map(|v| {
                                v.replace('\\', "\\\\")
                                    .replace('\t', "\\t")
                                    .replace('\n', "\\n")
                            })
                            .collect::<Vec<_>>()
                            .join("\t")
                    });
                match content {
                    Some(tsv) => {
                        state.ui.yank_flash = Some(YankFlash {
                            row: row_idx,
                            col: None,
                            until: now + Duration::from_millis(200),
                        });
                        Some(vec![Effect::CopyToClipboard {
                            content: tsv,
                            on_success: Some(Action::CellCopied),
                            on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                        }])
                    }
                    None => {
                        state
                            .messages
                            .set_error_at("Row index out of bounds".into(), now);
                        Some(vec![])
                    }
                }
            } else {
                Some(vec![])
            }
        }
        Action::ResultDeleteOperatorPending => {
            state.ui.delete_op_pending = true;
            Some(vec![])
        }
        Action::StageRowForDelete => {
            if state.ui.result_selection.mode() == crate::app::ui_state::ResultNavMode::RowActive
                && let Some(row_idx) = state.ui.result_selection.row()
            {
                state.ui.staged_delete_rows.insert(row_idx);
            }
            Some(vec![])
        }
        Action::UnstageLastStagedRow => {
            if let Some(&last) = state.ui.staged_delete_rows.iter().next_back() {
                state.ui.staged_delete_rows.remove(&last);
            }
            Some(vec![])
        }
        Action::ClearStagedDeletes => {
            state.ui.staged_delete_rows.clear();
            Some(vec![])
        }
        Action::CellCopied => Some(vec![]),
        Action::ResultEnterCellEdit => match editable_cell_context(state) {
            Ok((row_idx, col_idx, value)) => {
                if state.cell_edit.row != Some(row_idx) || state.cell_edit.col != Some(col_idx) {
                    state.cell_edit.begin(row_idx, col_idx, value);
                    state.pending_write_preview = None;
                }
                state.ui.input_mode = InputMode::CellEdit;
                Some(vec![])
            }
            Err(reason) => {
                state.messages.set_error_at(reason, now);
                Some(vec![])
            }
        },
        Action::ResultCancelCellEdit => {
            state.pending_write_preview = None;
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ResultDiscardCellEdit => {
            state.cell_edit.clear();
            state.pending_write_preview = None;
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ResultCellEditInput(c) => {
            state.cell_edit.input.insert_char(*c);
            Some(vec![])
        }
        Action::ResultCellEditBackspace => {
            state.cell_edit.input.backspace();
            Some(vec![])
        }
        Action::ResultCellEditDelete => {
            state.cell_edit.input.delete();
            Some(vec![])
        }
        Action::ResultCellEditMoveCursor(m) => {
            state.cell_edit.input.move_cursor(*m);
            Some(vec![])
        }
        Action::CopyFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }

        Action::ResultNextPage | Action::ResultPrevPage => {
            state.ui.result_selection.reset();
            state.cell_edit.clear();
            state.ui.staged_delete_rows.clear();
            state.pending_write_preview = None;
            None // Let the query reducer handle the actual page change
        }

        Action::ConnectionListSelectNext => {
            let len = state.connection_list_items().len();
            let next = state.ui.connection_list_selected + 1;
            if next < len {
                state.ui.set_connection_list_selection(Some(next));
            }
            Some(vec![])
        }
        Action::ConnectionListSelectPrevious => {
            if state.ui.connection_list_selected > 0 {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected - 1));
            }
            Some(vec![])
        }
        Action::ConnectionsLoaded(ConnectionsLoadedPayload {
            profiles,
            services,
            service_file_path,
            profile_load_warning,
            service_load_warning,
        }) => {
            let mut sorted = profiles.clone();
            sorted.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.set_connections_and_services(sorted, services.clone());
            state.runtime.service_file_path = service_file_path.clone();

            if let Some(warning) = profile_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }
            if let Some(warning) = service_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }

            let list_len = state.connection_list_items().len();
            if list_len == 0 {
                state.ui.set_connection_list_selection(Some(0));
            } else if state.ui.connection_list_selected >= list_len {
                state
                    .ui
                    .set_connection_list_selection(Some(list_len.saturating_sub(1)));
            } else {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected));
            }
            Some(vec![])
        }
        Action::ConfirmConnectionSelection => {
            use crate::app::connection_list::ConnectionListItem;
            let selected_idx = state.ui.connection_list_selected;

            let effect = match state.connection_list_items().get(selected_idx) {
                Some(ConnectionListItem::Profile(i)) => state
                    .connections()
                    .get(*i)
                    .filter(|c| state.runtime.active_connection_id.as_ref() != Some(&c.id))
                    .map(|_| Effect::SwitchConnection {
                        connection_index: *i,
                    }),
                Some(ConnectionListItem::Service(i)) => {
                    Some(Effect::SwitchToService { service_index: *i })
                }
                _ => None,
            };

            state.ui.input_mode = InputMode::Normal;

            match effect {
                Some(e) => Some(vec![e]),
                None => Some(vec![]),
            }
        }

        // Result history navigation
        Action::OpenResultHistory => {
            let len = state.query.result_history.len();
            if len == 0 {
                return Some(vec![]);
            }
            state.query.history_index = Some(len - 1);
            reset_result_view(state);
            Some(vec![])
        }
        Action::HistoryOlder => {
            if let Some(idx) = state.query.history_index
                && idx > 0
            {
                state.query.history_index = Some(idx - 1);
                reset_result_view(state);
            }
            Some(vec![])
        }
        Action::HistoryNewer => {
            if let Some(idx) = state.query.history_index {
                let len = state.query.result_history.len();
                if idx + 1 < len {
                    state.query.history_index = Some(idx + 1);
                    reset_result_view(state);
                }
                // At newest: no-op (use ^H to exit history)
            }
            Some(vec![])
        }
        Action::ExitResultHistory => {
            state.query.history_index = None;
            reset_result_view(state);
            Some(vec![])
        }

        _ => None,
    }
}

fn reset_result_view(state: &mut AppState) {
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.ui.result_selection.reset();
    state.cell_edit.clear();
    state.ui.staged_delete_rows.clear();
    state.pending_write_preview = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::Effect;
    use crate::app::services::AppServices;
    use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};
    use std::time::Instant;

    fn create_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: ConnectionId::new(),
            name: ConnectionName::new(name).unwrap(),
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            ssl_mode: SslMode::Prefer,
        }
    }

    mod paste {
        use super::*;

        #[test]
        fn paste_in_table_picker_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("hello".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;

            reduce_navigation(
                &mut state,
                &Action::Paste("hel\nlo\r\n".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_resets_selection() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.picker_selected = 5;

            reduce_navigation(
                &mut state,
                &Action::Paste("x".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.picker_selected, 0);
        }

        #[test]
        fn paste_in_command_line_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CommandLine;

            reduce_navigation(
                &mut state,
                &Action::Paste("quit".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_command_line_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CommandLine;

            reduce_navigation(
                &mut state,
                &Action::Paste("qu\nit".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_normal_mode_returns_none() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::Normal;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("text".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_none());
        }

        #[test]
        fn paste_in_er_table_picker_appends_to_er_filter() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ErTablePicker;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("public.users".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.er_filter_input, "public.users");
            assert_eq!(state.ui.er_picker_selected, 0);
        }

        #[test]
        fn paste_in_er_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ErTablePicker;

            reduce_navigation(
                &mut state,
                &Action::Paste("public\n.users\r\n".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.er_filter_input, "public.users");
        }
    }

    mod connection_list_navigation {
        use super::*;

        fn setup_profiles(state: &mut AppState, count: usize) {
            let names: Vec<String> = (1..=count).map(|i| format!("conn{}", i)).collect();
            let profiles = names.iter().map(|n| create_test_profile(n)).collect();
            state.set_connections(profiles);
        }

        #[test]
        fn select_next_increments_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 3);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_next_stops_at_last() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_previous_decrements_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn select_previous_stops_at_first() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 1);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }
    }

    mod connections_loaded {
        use super::*;

        #[test]
        fn sorts_connections_by_name_case_insensitive() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![
                create_test_profile("Zebra"),
                create_test_profile("alpha"),
                create_test_profile("Beta"),
            ];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.connections()[0].display_name(), "alpha");
            assert_eq!(state.connections()[1].display_name(), "Beta");
            assert_eq!(state.connections()[2].display_name(), "Zebra");
        }

        #[test]
        fn initializes_selection_when_not_empty() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![create_test_profile("conn1")];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn stores_service_file_path_in_runtime() {
            let mut state = AppState::new("test".to_string());
            let path = std::path::PathBuf::from("/etc/pg_service.conf");

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: Some(path.clone()),
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.runtime.service_file_path, Some(path));
        }

        #[test]
        fn service_load_warning_sets_error_message() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: Some("parse error at line 5".to_string()),
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.messages.last_error.is_some());
        }
    }

    mod confirm_connection_selection {
        use super::*;
        use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

        fn create_test_profile_with_id(name: &str, id: ConnectionId) -> ConnectionProfile {
            ConnectionProfile {
                id,
                name: ConnectionName::new(name).unwrap(),
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: SslMode::Prefer,
            }
        }

        #[test]
        fn different_connection_dispatches_switch_effect() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.runtime.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.iter().any(
                |e| matches!(e, Effect::SwitchConnection { connection_index } if *connection_index == 1)
            ));
        }

        #[test]
        fn stays_on_same_connection_returns_to_tables() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.runtime.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }

        #[test]
        fn empty_connections_returns_empty_effects() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }

        #[test]
        fn from_selector_mode_switches_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.runtime.active_connection_id = Some(active_id);
            state.ui.input_mode = InputMode::ConnectionSelector;
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::Normal);

            let effects = effects.unwrap();
            assert!(effects.iter().any(
                |e| matches!(e, Effect::SwitchConnection { connection_index } if *connection_index == 1)
            ));
        }

        #[test]
        fn from_selector_same_connection_returns_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.runtime.active_connection_id = Some(active_id);
            state.ui.input_mode = InputMode::ConnectionSelector;
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::Normal);

            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }
    }

    mod inspector_scroll_top_bottom {
        use super::*;
        use crate::domain::{Column, Table};

        fn state_with_table_detail(columns: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;
            state.ui.inspector_tab = crate::app::inspector_tab::InspectorTab::Columns;
            let cols: Vec<Column> = (0..columns)
                .map(|i| Column {
                    name: format!("col_{}", i),
                    data_type: "text".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: i as i32,
                })
                .collect();
            state.cache.table_detail = Some(Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                owner: None,
                columns: cols,
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: Some(0),
                comment: None,
            });
            state
        }

        #[test]
        fn inspector_scroll_top_resets_to_zero() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollTop,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 0;
            let visible = state.inspector_visible_rows(); // 10 - 5 = 5
            let expected_max = 20_usize.saturating_sub(visible);

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollBottom,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollBottom,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }
    }

    mod result_page_scroll {
        use super::*;
        use std::sync::Arc;

        fn state_with_result_rows(rows: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.result_pane_height = pane_height;
            let result_rows: Vec<Vec<String>> = (0..rows).map(|i| vec![format!("{}", i)]).collect();
            let row_count = result_rows.len();
            state.query.current_result = Some(Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns: vec!["id".to_string()],
                rows: result_rows,
                row_count,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: crate::domain::QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state
        }

        #[test]
        fn half_page_down_from_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 25 - 5 = 20, half = 10
            let effects = reduce_navigation(
                &mut state,
                &Action::ResultScrollHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn half_page_up_from_middle() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 50;

            reduce_navigation(
                &mut state,
                &Action::ResultScrollHalfPageUp,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.result_scroll_offset, 40);
        }

        #[test]
        fn full_page_down_clamped_at_max() {
            let mut state = state_with_result_rows(30, 25);
            // visible = 20, max_scroll = 30-20 = 10
            state.ui.result_scroll_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::ResultScrollFullPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            // delta=20, 5+20=25, clamped to 10
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn full_page_up_clamped_at_zero() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::ResultScrollFullPageUp,
                &AppServices::stub(),
                Instant::now(),
            );

            // delta=20, saturating_sub(5,20) = 0
            assert_eq!(state.ui.result_scroll_offset, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_result_rows(100, 0);
            // visible = 0, delta = max(0/2,1) = 1
            reduce_navigation(
                &mut state,
                &Action::ResultScrollHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.result_scroll_offset, 1);
        }
    }

    mod explorer_page_scroll {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn state_with_tables(count: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = pane_height;
            state.ui.focused_pane = FocusedPane::Explorer;
            let tables: Vec<TableSummary> = (0..count)
                .map(|i| {
                    TableSummary::new("public".to_string(), format!("table_{}", i), Some(0), false)
                })
                .collect();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            });
            state.ui.set_explorer_selection(Some(0));
            state
        }

        #[test]
        fn half_page_down_jumps_by_correct_delta() {
            let mut state = state_with_tables(50, 23);
            // explorer_visible_items = 23-3 = 20, half = 10
            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
        }

        #[test]
        fn half_page_down_clamped_at_last() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(45));

            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 49);
        }

        #[test]
        fn half_page_up_clamped_at_zero() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(3));

            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageUp,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn full_page_down_jumps_by_visible() {
            let mut state = state_with_tables(50, 23);
            // delta = 20
            reduce_navigation(
                &mut state,
                &Action::SelectFullPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 20);
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = 23;

            let effects = reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_tables(50, 0);
            // explorer_visible_items = 0, delta = max(0/2,1) = 1
            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 1);
        }
    }

    mod cell_yank {
        use super::*;
        use std::sync::Arc;

        fn state_with_grid(rows: usize, cols: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            let columns: Vec<String> = (0..cols).map(|c| format!("col_{}", c)).collect();
            let result_rows: Vec<Vec<String>> = (0..rows)
                .map(|r| (0..cols).map(|c| format!("r{}c{}", r, c)).collect())
                .collect();
            let row_count = result_rows.len();
            state.query.current_result = Some(Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns,
                rows: result_rows,
                row_count,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: crate::domain::QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state
        }

        #[test]
        fn out_of_bounds_row_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.ui.result_selection.enter_row(10);
            state.ui.result_selection.enter_cell(0);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn out_of_bounds_col_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(10);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn valid_cell_emits_copy_effect() {
            let mut state = state_with_grid(3, 3);
            state.ui.result_selection.enter_row(1);
            state.ui.result_selection.enter_cell(2);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "r1c2");
                }
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn no_selection_is_noop() {
            let mut state = state_with_grid(3, 3);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_none());
        }
    }

    mod row_yank {
        use super::*;
        use std::sync::Arc;

        fn state_with_row(values: Vec<&str>) -> AppState {
            let mut state = AppState::new("test".to_string());
            let columns: Vec<String> = (0..values.len()).map(|c| format!("col_{}", c)).collect();
            let rows = vec![values.iter().map(|v| v.to_string()).collect()];
            state.query.current_result = Some(Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns,
                rows,
                row_count: 1,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: crate::domain::QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state
        }

        #[test]
        fn row_yank_emits_tsv_copy_effect() {
            let mut state = state_with_row(vec!["v0", "v1", "v2"]);
            state.ui.result_selection.enter_row(0);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "v0\tv1\tv2");
                }
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_escapes_tab_and_newline() {
            let mut state = state_with_row(vec!["a\tb", "c\nd"]);
            state.ui.result_selection.enter_row(0);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "a\\tb\tc\\nd");
                }
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_escapes_backslash() {
            let mut state = state_with_row(vec!["a\\b"]);
            state.ui.result_selection.enter_row(0);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "a\\\\b");
                }
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_out_of_bounds_sets_error() {
            let mut state = state_with_row(vec!["val"]);
            state.ui.result_selection.enter_row(99);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn row_yank_no_selection_is_noop() {
            let mut state = state_with_row(vec!["val"]);

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn yank_op_pending_sets_flag() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::ResultRowYankOperatorPending,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(state.ui.yank_op_pending);
        }
    }

    mod cell_edit_entry_guardrails {
        use super::*;
        use crate::domain::{QueryResult, QuerySource, Table};
        use std::sync::Arc;

        fn minimal_users_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        fn preview_state_with_selection() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.query.current_result = Some(Arc::new(QueryResult {
                query: String::new(),
                columns: vec!["id".to_string(), "name".to_string()],
                rows: vec![vec!["1".to_string(), "alice".to_string()]],
                row_count: 1,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);
            state
        }

        #[test]
        fn re_entering_same_cell_with_pending_draft_preserves_draft() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(minimal_users_table());
            state.cell_edit.begin(0, 1, "alice".to_string());
            state.cell_edit.input.set_content("modified".to_string());
            state.ui.input_mode = InputMode::Normal;

            reduce_navigation(
                &mut state,
                &Action::ResultEnterCellEdit,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(state.cell_edit.draft_value(), "modified");
        }

        #[test]
        fn entering_different_cell_resets_draft() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(minimal_users_table());
            state.cell_edit.begin(0, 99, "stale".to_string());
            state
                .cell_edit
                .input
                .set_content("stale-modified".to_string());

            reduce_navigation(
                &mut state,
                &Action::ResultEnterCellEdit,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(state.cell_edit.col, Some(1));
            assert_eq!(state.cell_edit.draft_value(), "alice");
        }

        #[test]
        fn stale_table_detail_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(Table {
                schema: "public".to_string(),
                name: "posts".to_string(),
                owner: None,
                columns: vec![],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            });

            let effects = reduce_navigation(
                &mut state,
                &Action::ResultEnterCellEdit,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Table metadata does not match current preview target")
            );
        }
    }

    mod row_delete {
        use super::*;
        use crate::domain::{Column, QueryResult, QuerySource, Table};
        use std::sync::Arc;

        fn base_state(
            pk: Option<Vec<&str>>,
            rows: Vec<Vec<&str>>,
            current_page: usize,
        ) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.cache.selection_generation = 7;
            state.query.pagination.current_page = current_page;
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.current_result = Some(Arc::new(QueryResult {
                query: "SELECT * FROM public.users".to_string(),
                columns: vec!["id".to_string(), "name".to_string()],
                row_count: rows.len(),
                rows: rows
                    .into_iter()
                    .map(|r| r.into_iter().map(|v| v.to_string()).collect())
                    .collect(),
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state.cache.table_detail = Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: pk.map(|cols| cols.into_iter().map(|c| c.to_string()).collect()),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            });
            state
        }

        #[test]
        fn dd_stages_active_row() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.ui.result_selection.enter_row(0);

            reduce_navigation(
                &mut state,
                &Action::StageRowForDelete,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.ui.staged_delete_rows.contains(&0));
        }

        #[test]
        fn dd_on_already_staged_row_is_noop() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.ui.result_selection.enter_row(0);
            state.ui.staged_delete_rows.insert(0);

            reduce_navigation(
                &mut state,
                &Action::StageRowForDelete,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.staged_delete_rows.len(), 1);
        }

        #[test]
        fn staging_requires_row_active_mode() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);

            reduce_navigation(
                &mut state,
                &Action::StageRowForDelete,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.ui.staged_delete_rows.is_empty());
        }

        #[test]
        fn u_unstages_last_staged_row() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.ui.staged_delete_rows.insert(0);
            state.ui.staged_delete_rows.insert(1);

            reduce_navigation(
                &mut state,
                &Action::UnstageLastStagedRow,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.staged_delete_rows.len(), 1);
            assert!(state.ui.staged_delete_rows.contains(&0));
        }

        #[test]
        fn clear_staged_deletes_removes_all() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.ui.staged_delete_rows.insert(0);
            state.ui.staged_delete_rows.insert(1);

            reduce_navigation(
                &mut state,
                &Action::ClearStagedDeletes,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.ui.staged_delete_rows.is_empty());
        }
    }

    mod ddl_yank {
        use super::*;
        use crate::app::ports::ddl_generator::DdlGenerator;
        use crate::domain::{Column, Table};
        use std::sync::Arc;

        struct FakeDdlGenerator;
        impl DdlGenerator for FakeDdlGenerator {
            fn generate_ddl(&self, table: &Table) -> String {
                format!("CREATE TABLE {}.{} ();", table.schema, table.name)
            }
        }

        fn fake_services() -> AppServices {
            let mut services = AppServices::stub();
            services.ddl_generator = Arc::new(FakeDdlGenerator);
            services
        }

        fn state_with_ddl_tab() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.cache.table_detail = Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: Some(vec!["id".to_string()]),
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: Some(0),
                comment: None,
            });
            state
        }

        #[test]
        fn ddl_yank_with_table_detail_returns_copy_effect() {
            let mut state = state_with_ddl_tab();

            let effects = reduce_navigation(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content.contains("CREATE TABLE"))
            );
        }

        #[test]
        fn ddl_yank_without_table_detail_returns_empty() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Ddl;
            // table_detail is None

            let effects = reduce_navigation(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert!(effects.is_empty());
        }

        #[test]
        fn ddl_yank_on_non_ddl_tab_returns_empty() {
            let mut state = state_with_ddl_tab();
            state.ui.inspector_tab = InspectorTab::Info;

            let effects = reduce_navigation(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert!(effects.is_empty());
        }
    }

    mod cell_edit_cursor_ops {
        use super::*;
        use crate::app::action::CursorMove;

        fn state_in_cell_edit(content: &str, cursor: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CellEdit;
            state.cell_edit.begin(0, 0, content.to_string());
            state.cell_edit.input.set_cursor(cursor);
            state
        }

        #[test]
        fn delete_removes_char_at_cursor() {
            let mut state = state_in_cell_edit("abcd", 1);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditDelete,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.draft_value(), "acd");
            assert_eq!(state.cell_edit.input.cursor(), 1);
        }

        #[test]
        fn delete_at_end_is_noop() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditDelete,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.draft_value(), "abc");
        }

        #[test]
        fn move_cursor_left_decrements() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Left),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 1);
        }

        #[test]
        fn move_cursor_right_increments() {
            let mut state = state_in_cell_edit("abc", 1);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Right),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 2);
        }

        #[test]
        fn move_cursor_home_jumps_to_start() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Home),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 0);
        }

        #[test]
        fn move_cursor_end_jumps_to_end() {
            let mut state = state_in_cell_edit("abc", 0);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::End),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 3);
        }

        #[test]
        fn input_inserts_at_cursor_not_at_end() {
            let mut state = state_in_cell_edit("ac", 1);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditInput('b'),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.draft_value(), "abc");
            assert_eq!(state.cell_edit.input.cursor(), 2);
        }

        #[test]
        fn backspace_removes_char_before_cursor() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce_navigation(
                &mut state,
                &Action::ResultCellEditBackspace,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.draft_value(), "ac");
            assert_eq!(state.cell_edit.input.cursor(), 1);
        }
    }

    mod result_history {
        use super::*;
        use crate::domain::{QueryResult, QuerySource};
        use std::sync::Arc;

        fn make_result(query: &str) -> Arc<QueryResult> {
            Arc::new(QueryResult::success(
                query.to_string(),
                vec!["col".to_string()],
                vec![vec!["val".to_string()]],
                10,
                QuerySource::Adhoc,
            ))
        }

        fn state_with_history(count: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            for i in 0..count {
                state
                    .query
                    .result_history
                    .push(make_result(&format!("SELECT {}", i + 1)));
            }
            state.query.current_result = Some(make_result("SELECT latest"));
            state
        }

        #[test]
        fn open_sets_index_to_newest() {
            let mut state = state_with_history(3);

            reduce_navigation(
                &mut state,
                &Action::OpenResultHistory,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, Some(2));
        }

        #[test]
        fn open_is_noop_when_history_empty() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::OpenResultHistory,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, None);
        }

        #[test]
        fn older_decrements_index() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(2);

            reduce_navigation(
                &mut state,
                &Action::HistoryOlder,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, Some(1));
        }

        #[test]
        fn older_clamps_at_zero() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(0);

            reduce_navigation(
                &mut state,
                &Action::HistoryOlder,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, Some(0));
        }

        #[test]
        fn newer_increments_index() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(0);

            reduce_navigation(
                &mut state,
                &Action::HistoryNewer,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, Some(1));
        }

        #[test]
        fn newer_at_last_is_noop() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(2);

            reduce_navigation(
                &mut state,
                &Action::HistoryNewer,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, Some(2));
        }

        #[test]
        fn exit_clears_index() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);

            reduce_navigation(
                &mut state,
                &Action::ExitResultHistory,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query.history_index, None);
        }

        #[test]
        fn navigation_resets_scroll_offset() {
            let mut state = state_with_history(3);
            state.ui.result_scroll_offset = 10;
            state.ui.result_horizontal_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::OpenResultHistory,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.result_scroll_offset, 0);
            assert_eq!(state.ui.result_horizontal_offset, 0);
        }
    }
}
