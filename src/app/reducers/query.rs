//! Query sub-reducer: query execution and command line.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::action::{Action, TableTarget};
use crate::app::command::{command_to_action, parse_command};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection, QueryStatus};
use crate::app::services::AppServices;
use crate::app::sql_modal_context::{AdhocSuccessSnapshot, SqlModalStatus};
use crate::app::state::AppState;
use crate::app::write_guardrails::{
    ColumnDiff, RiskLevel, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::write_update::{build_pk_pairs, escape_preview_value};
use crate::domain::{QueryResult, QuerySource};

use super::helpers::{build_bulk_delete_preview, editable_preview_base};

fn build_update_preview(state: &AppState, services: &AppServices) -> Result<WritePreview, String> {
    if !state.cell_edit.is_active() {
        return Err("No active cell edit session".to_string());
    }

    let (result, pk_cols) = editable_preview_base(state)?;

    let row_idx = state
        .cell_edit
        .row
        .ok_or_else(|| "No row selected for edit".to_string())?;
    let col_idx = state
        .cell_edit
        .col
        .ok_or_else(|| "No column selected for edit".to_string())?;

    let row = result
        .rows
        .get(row_idx)
        .ok_or_else(|| "Row index out of bounds".to_string())?;
    let column_name = result
        .columns
        .get(col_idx)
        .ok_or_else(|| "Column index out of bounds".to_string())?
        .clone();

    if pk_cols.iter().any(|pk| pk == &column_name) {
        return Err("Primary key columns are read-only".to_string());
    }

    let pk_pairs = build_pk_pairs(&result.columns, row, pk_cols);
    let target = crate::app::write_guardrails::TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: pk_pairs.clone().unwrap_or_default(),
    };
    let has_where = pk_pairs.as_ref().is_some_and(|pairs| !pairs.is_empty());
    let has_stable_row_identity = pk_pairs.is_some();
    let guardrail = evaluate_guardrails(has_where, has_stable_row_identity, Some(target.clone()));
    if guardrail.blocked {
        let reason = guardrail
            .reason
            .clone()
            .unwrap_or_else(|| "Write blocked by guardrails".to_string());
        return Err(reason);
    }

    let sql = services.sql_dialect.build_update_sql(
        &target.schema,
        &target.table,
        &column_name,
        state.cell_edit.draft_value(),
        &target.key_values,
    );
    let preview = WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: target,
        diff: vec![ColumnDiff {
            column: column_name,
            before: state.cell_edit.original_value.clone(),
            after: state.cell_edit.draft_value().to_string(),
        }],
        guardrail,
    };
    Ok(preview)
}

fn build_write_preview_fallback_message(preview: &WritePreview) -> String {
    // `pending_write_preview` drives rich rendering; this is stored only as a fallback.
    let mut lines = Vec::new();
    if preview.guardrail.risk_level != RiskLevel::Low {
        lines.push(format!("Risk: {}", preview.guardrail.risk_level.as_str()));
    }
    match preview.operation {
        WriteOperation::Update => {
            lines.push(preview.diff.first().map_or_else(
                || "(no changes)".to_string(),
                |d| {
                    format!(
                        "{}: \"{}\" -> \"{}\"",
                        d.column,
                        escape_preview_value(&d.before),
                        escape_preview_value(&d.after)
                    )
                },
            ));
        }
        WriteOperation::Delete => {
            lines.push(format!(
                "Target: {}",
                preview.target_summary.format_compact()
            ));
        }
    }
    lines.join("\n")
}

/// Handles query execution and command line actions.
/// Returns Some(effects) if action was handled, None otherwise.
fn try_adhoc_refresh(state: &mut AppState, result: &QueryResult) -> Vec<Effect> {
    if result.source != QuerySource::Adhoc || result.is_error() {
        return vec![];
    }
    let Some(tag) = &result.command_tag else {
        return vec![];
    };
    if !tag.needs_refresh() {
        return vec![];
    }
    let Some(dsn) = state.runtime.dsn.clone() else {
        return vec![];
    };

    let mut effects = vec![];

    if tag.is_schema_modifying() {
        // Skip ExecutePreview here: the selected table may have been
        // dropped. MetadataLoaded handles preview refresh after
        // metadata arrives.
        state.sql_modal.prefetch_started = false;
        state.sql_modal.prefetch_queue.clear();
        state.sql_modal.prefetching_tables.clear();
        state.sql_modal.failed_prefetch_tables.clear();
        state.cache.table_detail = None;

        effects.push(Effect::CacheInvalidate { dsn: dsn.clone() });
        effects.push(Effect::ClearCompletionEngineCache);
        effects.push(Effect::FetchMetadata { dsn });
    } else if !state.query.pagination.table.is_empty() {
        let page = state.query.pagination.current_page;
        effects.push(Effect::ExecutePreview {
            dsn,
            schema: state.query.pagination.schema.clone(),
            table: state.query.pagination.table.clone(),
            generation: state.cache.selection_generation,
            limit: PREVIEW_PAGE_SIZE,
            offset: page * PREVIEW_PAGE_SIZE,
            target_page: page,
        });
    }

    effects
}

pub fn reduce_query(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::QueryCompleted {
            result,
            generation,
            target_page,
        } => {
            if *generation == 0 || *generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.ui.result_selection.reset();
                state.cell_edit.clear();
                state.pending_write_preview = None;
                state.query.result_highlight_until = Some(now + Duration::from_millis(500));
                state.query.history_index = None;

                if result.source == QuerySource::Adhoc {
                    if result.is_error() {
                        state.sql_modal.status = SqlModalStatus::Error;
                    } else {
                        state.sql_modal.status = SqlModalStatus::Success;
                        state.sql_modal.last_adhoc_success = Some(AdhocSuccessSnapshot {
                            command_tag: result.command_tag.clone(),
                            row_count: result.row_count,
                            execution_time_ms: result.execution_time_ms,
                        });
                    }
                }

                if result.source == QuerySource::Adhoc && !result.is_error() {
                    state.query.result_history.push(Arc::clone(result));
                }

                if let Some(page) = target_page {
                    state.query.pagination.current_page = *page;
                    if result.rows.len() < PREVIEW_PAGE_SIZE {
                        state.query.pagination.reached_end = true;
                    }
                }

                state.query.current_result = Some(Arc::clone(result));

                if result.source == QuerySource::Preview {
                    match state.query.post_delete_row_selection {
                        PostDeleteRowSelection::Keep => {}
                        PostDeleteRowSelection::Clear => {
                            state.ui.result_selection.reset();
                        }
                        PostDeleteRowSelection::Select(row) => {
                            if !result.rows.is_empty() {
                                let clamped = row.min(result.rows.len() - 1);
                                state.ui.result_selection.enter_row(clamped);

                                let visible = state.result_visible_rows();
                                if visible > 0 && clamped >= visible {
                                    state.ui.result_scroll_offset = clamped - visible + 1;
                                }
                            }
                        }
                    }
                    state.query.post_delete_row_selection = PostDeleteRowSelection::Keep;
                }

                Some(try_adhoc_refresh(state, result))
            } else {
                Some(vec![])
            }
        }
        Action::QueryFailed(error, generation) => {
            if *generation == 0 || *generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_selection.reset();
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.cell_edit.clear();
                state.pending_write_preview = None;
                state.set_error(error.clone());
                if state.ui.input_mode == InputMode::SqlModal {
                    state.sql_modal.status = SqlModalStatus::Error;
                    let error_result = Arc::new(QueryResult::error(
                        state.sql_modal.content.clone(),
                        error.clone(),
                        0,
                        QuerySource::Adhoc,
                    ));
                    state.query.current_result = Some(error_result);
                }
                state.query.post_delete_row_selection = PostDeleteRowSelection::Keep;
                state.query.pending_delete_refresh_target = None;
            }
            Some(vec![])
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.ui.input_mode = state.ui.command_line_return_mode;
            state.ui.command_line_return_mode = InputMode::Normal;
            state.command_line_input.clear();

            Some(match follow_up {
                Action::Quit => {
                    state.should_quit = true;
                    vec![]
                }
                Action::OpenHelp => {
                    state.ui.input_mode = InputMode::Help;
                    vec![]
                }
                Action::OpenSqlModal => {
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.status = SqlModalStatus::Editing;
                    if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                        vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
                    } else {
                        vec![]
                    }
                }
                Action::ErOpenDiagram => {
                    vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])]
                }
                Action::SubmitCellEditWrite => {
                    vec![Effect::DispatchActions(vec![Action::SubmitCellEditWrite])]
                }
                _ => vec![],
            })
        }

        Action::ExecutePreview(TableTarget {
            schema,
            table,
            generation,
        }) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);

                // Initialize pagination for this preview
                state.query.pagination.reset();
                state.query.pagination.schema = schema.clone();
                state.query.pagination.table = table.clone();

                // Look up row_count_estimate from table detail or table summaries
                let row_estimate = state
                    .cache
                    .table_detail
                    .as_ref()
                    .and_then(|d| d.row_count_estimate)
                    .or_else(|| {
                        state.tables().iter().find_map(|t| {
                            if t.schema == *schema && t.name == *table {
                                t.row_count_estimate
                            } else {
                                None
                            }
                        })
                    });
                state.query.pagination.total_rows_estimate = row_estimate;

                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: 0,
                    target_page: 0,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                Some(vec![Effect::ExecuteAdhoc {
                    dsn: dsn.clone(),
                    query: query.clone(),
                }])
            } else {
                Some(vec![])
            }
        }

        Action::SubmitCellEditWrite => {
            if !state.ui.staged_delete_rows.is_empty() {
                match build_bulk_delete_preview(state, services) {
                    Ok((preview, target_page, target_row)) => {
                        let staged_count = state.ui.staged_delete_rows.len();
                        state.query.pending_delete_refresh_target =
                            Some((target_page, target_row, staged_count));
                        return Some(vec![Effect::DispatchActions(vec![
                            Action::OpenWritePreviewConfirm(Box::new(preview)),
                        ])]);
                    }
                    Err(msg) => {
                        state.messages.set_error_at(msg, now);
                        return Some(vec![]);
                    }
                }
            }

            if !state.cell_edit.is_active() {
                state
                    .messages
                    .set_error_at("No active cell edit session".to_string(), now);
                return Some(vec![]);
            }
            if state.query.status != QueryStatus::Idle {
                state.messages.set_error_at(
                    "Write is unavailable while query is running".to_string(),
                    now,
                );
                return Some(vec![]);
            }

            match build_update_preview(state, services) {
                Ok(preview) => Some(vec![Effect::DispatchActions(vec![
                    Action::OpenWritePreviewConfirm(Box::new(preview)),
                ])]),
                Err(msg) => {
                    state.messages.set_error_at(msg, now);
                    Some(vec![])
                }
            }
        }

        Action::OpenWritePreviewConfirm(preview) => {
            state.pending_write_preview = Some((**preview).clone());
            let operation = preview.operation;
            let caller_mode = state.ui.input_mode;
            let (title, return_mode) = match operation {
                WriteOperation::Update => {
                    state.query.pending_delete_refresh_target = None;
                    (
                        format!("Confirm UPDATE: {}", preview.target_summary.table),
                        caller_mode,
                    )
                }
                WriteOperation::Delete => {
                    let n = state
                        .query
                        .pending_delete_refresh_target
                        .as_ref()
                        .map(|(_, _, count)| *count)
                        .unwrap_or(1);
                    (
                        format!(
                            "Confirm DELETE: {} {} from {}",
                            n,
                            if n == 1 { "row" } else { "rows" },
                            preview.target_summary.table
                        ),
                        InputMode::Normal,
                    )
                }
            };

            state.confirm_dialog.title = title;
            state.confirm_dialog.message = build_write_preview_fallback_message(preview);
            state.confirm_dialog.on_confirm = if preview.guardrail.blocked {
                Action::None
            } else {
                Action::ExecuteWrite(preview.sql.clone())
            };
            state.confirm_dialog.on_cancel = Action::None;
            state.confirm_dialog.return_mode = return_mode;
            state.ui.input_mode = InputMode::ConfirmDialog;

            Some(vec![])
        }

        Action::ExecuteWrite(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                Some(vec![Effect::ExecuteWrite {
                    dsn: dsn.clone(),
                    query: query.clone(),
                }])
            } else {
                state
                    .messages
                    .set_error_at("No active connection".to_string(), now);
                Some(vec![])
            }
        }

        Action::ExecuteWriteSucceeded { affected_rows } => {
            state.query.status = QueryStatus::Idle;
            state.query.start_time = None;
            let operation = state
                .pending_write_preview
                .as_ref()
                .map(|p| p.operation)
                .unwrap_or(WriteOperation::Update);
            state.pending_write_preview = None;
            match operation {
                WriteOperation::Update => {
                    if *affected_rows != 1 {
                        state.messages.set_error_at(
                            format!("UPDATE expected 1 row, but affected {} rows", affected_rows),
                            now,
                        );
                        state.ui.input_mode = InputMode::CellEdit;
                        return Some(vec![]);
                    }

                    state
                        .messages
                        .set_success_at("Updated 1 row".to_string(), now);
                    state.cell_edit.clear();
                    state.ui.input_mode = InputMode::Normal;

                    if let Some(dsn) = &state.runtime.dsn {
                        let page = state.query.pagination.current_page;
                        state.query.status = QueryStatus::Running;
                        state.query.start_time = Some(now);
                        Some(vec![Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema: state.query.pagination.schema.clone(),
                            table: state.query.pagination.table.clone(),
                            generation: state.cache.selection_generation,
                            limit: PREVIEW_PAGE_SIZE,
                            offset: page * PREVIEW_PAGE_SIZE,
                            target_page: page,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
                WriteOperation::Delete => {
                    let (target_page, target_row, expected) = state
                        .query
                        .pending_delete_refresh_target
                        .take()
                        .unwrap_or((state.query.pagination.current_page, None, 1));

                    let row_word = |n: usize| if n == 1 { "row" } else { "rows" };
                    if *affected_rows != expected {
                        state.messages.set_error_at(
                            format!(
                                "DELETE expected {} {}, but affected {} {}",
                                expected,
                                row_word(expected),
                                affected_rows,
                                row_word(*affected_rows),
                            ),
                            now,
                        );
                    } else {
                        state.messages.set_success_at(
                            format!("Deleted {} {}", expected, row_word(expected)),
                            now,
                        );
                    }
                    state.cell_edit.clear();
                    state.ui.staged_delete_rows.clear();
                    state.ui.input_mode = InputMode::Normal;

                    state.query.post_delete_row_selection = target_row
                        .map(PostDeleteRowSelection::Select)
                        .unwrap_or(PostDeleteRowSelection::Clear);

                    if let Some(dsn) = &state.runtime.dsn {
                        state.query.status = QueryStatus::Running;
                        state.query.start_time = Some(now);
                        state.query.pagination.reached_end = false;
                        Some(vec![Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema: state.query.pagination.schema.clone(),
                            table: state.query.pagination.table.clone(),
                            generation: state.cache.selection_generation,
                            limit: PREVIEW_PAGE_SIZE,
                            offset: target_page * PREVIEW_PAGE_SIZE,
                            target_page,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
            }
        }

        Action::ExecuteWriteFailed(error) => {
            state.query.status = QueryStatus::Idle;
            state.query.start_time = None;
            let operation = state
                .pending_write_preview
                .as_ref()
                .map(|p| p.operation)
                .unwrap_or(WriteOperation::Update);
            state.pending_write_preview = None;
            state.query.pending_delete_refresh_target = None;
            state.messages.set_error_at(error.clone(), now);
            state.ui.input_mode = match operation {
                WriteOperation::Update => InputMode::CellEdit,
                WriteOperation::Delete => InputMode::Normal,
            };
            Some(vec![])
        }

        // ── CSV Export ──────────────────────────────────────────────
        Action::RequestCsvExport => {
            let result = match &state.query.current_result {
                Some(r) if !r.is_error() => r,
                _ => return Some(vec![]),
            };
            let dsn = match &state.runtime.dsn {
                Some(d) => d.clone(),
                None => return Some(vec![]),
            };

            let export_query = result.query.clone();
            let file_name = match result.source {
                QuerySource::Preview => {
                    let table = &state.query.pagination.table;
                    table
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '_' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect()
                }
                QuerySource::Adhoc => "adhoc".to_string(),
            };

            let stripped = export_query.trim_end().trim_end_matches(';').to_string();
            let count_query = format!("SELECT COUNT(*) FROM ({}) AS _export_count", stripped);

            Some(vec![Effect::CountRowsForExport {
                dsn,
                count_query,
                export_query,
                file_name,
            }])
        }

        Action::CsvExportRowsCounted {
            row_count,
            export_query,
            file_name,
        } => {
            const LARGE_EXPORT_THRESHOLD: usize = 100_000;

            let needs_confirm = match row_count {
                Some(n) => *n > LARGE_EXPORT_THRESHOLD,
                None => true,
            };

            if needs_confirm {
                let msg = match row_count {
                    Some(n) => format!("Export {} rows to CSV? This may take a while.", n),
                    None => "Row count unknown. Export to CSV?".to_string(),
                };
                state.confirm_dialog.title = "Confirm CSV Export".to_string();
                state.confirm_dialog.message = msg;
                state.confirm_dialog.on_confirm = Action::ExecuteCsvExport {
                    export_query: export_query.clone(),
                    file_name: file_name.clone(),
                    row_count: *row_count,
                };
                state.confirm_dialog.on_cancel = Action::None;
                state.confirm_dialog.return_mode = InputMode::Normal;
                state.ui.input_mode = InputMode::ConfirmDialog;
                Some(vec![])
            } else {
                let dsn = match &state.runtime.dsn {
                    Some(d) => d.clone(),
                    None => return Some(vec![]),
                };
                Some(vec![Effect::ExportCsv {
                    dsn,
                    query: export_query.clone(),
                    file_name: file_name.clone(),
                    row_count: *row_count,
                }])
            }
        }

        Action::ExecuteCsvExport {
            export_query,
            file_name,
            row_count,
        } => {
            let dsn = match &state.runtime.dsn {
                Some(d) => d.clone(),
                None => return Some(vec![]),
            };
            Some(vec![Effect::ExportCsv {
                dsn,
                query: export_query.clone(),
                file_name: file_name.clone(),
                row_count: *row_count,
            }])
        }

        Action::CsvExportSucceeded { path, row_count } => {
            let msg = match row_count {
                Some(n) => format!("Exported {} rows → {}", n, path),
                None => format!("Exported → {}", path),
            };
            state.messages.set_success_at(msg, now);
            let folder = std::path::Path::new(path)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            Some(vec![Effect::OpenFolder { path: folder }])
        }

        Action::CsvExportFailed(error) => {
            state.messages.set_error_at(error.clone(), now);
            Some(vec![])
        }

        Action::ResultNextPage => {
            let is_preview = state
                .query
                .current_result
                .as_ref()
                .is_some_and(|r| r.source == QuerySource::Preview);
            if state.query.status != QueryStatus::Idle || !is_preview {
                return Some(vec![]);
            }
            if !state.query.pagination.can_next() {
                return Some(vec![]);
            }
            if let Some(dsn) = &state.runtime.dsn {
                let next_page = state.query.pagination.current_page + 1;
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.cache.selection_generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: next_page * PREVIEW_PAGE_SIZE,
                    target_page: next_page,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ResultPrevPage => {
            let is_preview = state
                .query
                .current_result
                .as_ref()
                .is_some_and(|r| r.source == QuerySource::Preview);
            if state.query.status != QueryStatus::Idle || !is_preview {
                return Some(vec![]);
            }
            if !state.query.pagination.can_prev() {
                return Some(vec![]);
            }
            if let Some(dsn) = &state.runtime.dsn {
                let prev_page = state.query.pagination.current_page - 1;
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(now);
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                // When going back, the page is not at the end anymore
                state.query.pagination.reached_end = false;
                Some(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.cache.selection_generation,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: prev_page * PREVIEW_PAGE_SIZE,
                    target_page: prev_page,
                }])
            } else {
                Some(vec![])
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::query_execution::PaginationState;
    use crate::domain::{
        Column, CommandTag, Index, IndexType, Table, Trigger, TriggerEvent, TriggerTiming,
    };

    fn create_test_state() -> AppState {
        let mut state = AppState::new("test_project".to_string());
        state.runtime.dsn = Some("postgres://localhost/test".to_string());
        state
    }

    fn preview_result(row_count: usize) -> Arc<QueryResult> {
        let rows: Vec<Vec<String>> = (0..row_count).map(|i| vec![i.to_string()]).collect();
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string()],
            rows,
            row_count,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
    }

    fn adhoc_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT 1".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Adhoc,
            error: None,
            command_tag: None,
        })
    }

    fn editable_preview_result() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![vec!["1".to_string(), "Alice".to_string()]],
            row_count: 1,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
    }

    fn users_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "name".to_string(),
                    data_type: "text".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![Index {
                name: "users_pkey".to_string(),
                columns: vec!["id".to_string()],
                is_unique: true,
                is_primary: true,
                index_type: IndexType::BTree,
                definition: None,
            }],
            rls: None,
            triggers: vec![Trigger {
                name: "trg".to_string(),
                timing: TriggerTiming::After,
                events: vec![TriggerEvent::Update],
                function_name: "f".to_string(),
                security_definer: false,
            }],
            row_count_estimate: None,
            comment: None,
        }
    }

    fn adhoc_result_with_tag(tag: CommandTag) -> Arc<QueryResult> {
        Arc::new(QueryResult {
            query: String::new(),
            columns: vec![],
            rows: vec![],
            row_count: 0,
            execution_time_ms: 5,
            executed_at: Instant::now(),
            source: QuerySource::Adhoc,
            error: None,
            command_tag: Some(tag),
        })
    }

    fn adhoc_error_result() -> Arc<QueryResult> {
        Arc::new(QueryResult::error(
            "BAD SQL".to_string(),
            "syntax error".to_string(),
            5,
            QuerySource::Adhoc,
        ))
    }

    fn state_with_table(schema: &str, table: &str) -> AppState {
        let mut state = create_test_state();
        state.query.pagination.schema = schema.to_string();
        state.query.pagination.table = table.to_string();
        state
    }

    mod next_page {
        use super::*;

        #[test]
        fn emits_effect_with_correct_offset() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination = PaginationState {
                current_page: 0,
                total_rows_estimate: Some(1500),
                reached_end: false,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultNextPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 1);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn noop_when_reached_end() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(100));
            state.query.pagination.reached_end = true;
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultNextPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_for_adhoc() {
            let mut state = create_test_state();
            state.query.current_result = Some(adhoc_result());
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultNextPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_when_running() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.status = QueryStatus::Running;
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultNextPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }
    }

    mod prev_page {
        use super::*;

        #[test]
        fn emits_effect_with_correct_offset() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination = PaginationState {
                current_page: 2,
                total_rows_estimate: Some(1500),
                reached_end: false,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultPrevPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 1);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn noop_on_first_page() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination.current_page = 0;
            let now = Instant::now();

            let effects = reduce_query(
                &mut state,
                &Action::ResultPrevPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }
    }

    mod execute_preview {
        use super::*;

        #[test]
        fn resets_pagination() {
            let mut state = create_test_state();
            state.query.pagination = PaginationState {
                current_page: 5,
                total_rows_estimate: Some(10000),
                reached_end: true,
                schema: "old_schema".to_string(),
                table: "old_table".to_string(),
            };
            let now = Instant::now();

            reduce_query(
                &mut state,
                &Action::ExecutePreview(TableTarget {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                }),
                now,
                &AppServices::stub(),
            );

            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
            assert_eq!(state.query.pagination.schema, "public");
            assert_eq!(state.query.pagination.table, "users");
        }
    }

    mod query_completed {
        use super::*;

        #[test]
        fn sets_page_and_reached_end() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            let result = preview_result(100); // Less than PAGE_SIZE
            let now = Instant::now();

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(2),
                },
                now,
                &AppServices::stub(),
            );

            assert_eq!(state.query.pagination.current_page, 2);
            assert!(state.query.pagination.reached_end);
        }

        #[test]
        fn does_not_set_reached_end_for_full_page() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            let result = preview_result(PREVIEW_PAGE_SIZE);
            let now = Instant::now();

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(0),
                },
                now,
                &AppServices::stub(),
            );

            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
        }

        #[test]
        fn adhoc_does_not_update_pagination() {
            let mut state = create_test_state();
            state.query.pagination.current_page = 3;
            let result = adhoc_result();
            let now = Instant::now();

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                now,
                &AppServices::stub(),
            );

            // Pagination unchanged for adhoc
            assert_eq!(state.query.pagination.current_page, 3);
        }
    }

    mod query_failed {
        use super::*;
        use crate::app::ui_state::ResultNavMode;

        #[test]
        fn resets_result_selection_and_offsets() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            state.ui.result_selection.enter_row(5);
            state.ui.result_selection.enter_cell(2);
            state.ui.result_scroll_offset = 10;
            state.ui.result_horizontal_offset = 3;

            reduce_query(
                &mut state,
                &Action::QueryFailed("error".to_string(), 1),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.result_selection.mode(), ResultNavMode::Scroll);
            assert_eq!(state.ui.result_scroll_offset, 0);
            assert_eq!(state.ui.result_horizontal_offset, 0);
        }
    }

    mod write_flow {
        use super::*;
        use crate::app::ports::SqlDialect;

        struct FakeSqlDialect;
        impl SqlDialect for FakeSqlDialect {
            fn build_update_sql(
                &self,
                schema: &str,
                table: &str,
                column: &str,
                new_value: &str,
                pk_pairs: &[(String, String)],
            ) -> String {
                let set_clause = format!("\"{}\" = '{}'", column, new_value);
                let where_clause: Vec<String> = pk_pairs
                    .iter()
                    .map(|(k, v)| format!("\"{}\" = '{}'", k, v))
                    .collect();
                format!(
                    "UPDATE \"{}\".\"{}\" SET {} WHERE {}",
                    schema,
                    table,
                    set_clause,
                    where_clause.join(" AND ")
                )
            }
            fn build_bulk_delete_sql(
                &self,
                _schema: &str,
                _table: &str,
                _pk_pairs_per_row: &[Vec<(String, String)>],
            ) -> String {
                String::new()
            }
        }

        fn fake_services() -> AppServices {
            AppServices {
                ddl_generator: AppServices::stub().ddl_generator,
                sql_dialect: std::sync::Arc::new(FakeSqlDialect),
            }
        }

        fn editable_state() -> AppState {
            let mut state = AppState::new("test_project".to_string());
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.query.current_result = Some(editable_preview_result());
            state.cache.table_detail = Some(users_table_detail());
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.ui.input_mode = InputMode::CellEdit;
            state.cell_edit.begin(0, 1, "Alice".to_string());
            state.cell_edit.input.set_content("Bob".to_string());
            state
        }

        #[test]
        fn write_requires_cell_edit_mode() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::Normal;
            // No cell_edit active

            let effects = reduce_query(
                &mut state,
                &Action::SubmitCellEditWrite,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(effects.unwrap().is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("No active cell edit session")
            );
        }

        #[test]
        fn write_requires_idle_query_status() {
            let mut state = editable_state();
            state.query.status = QueryStatus::Running;

            let effects = reduce_query(
                &mut state,
                &Action::SubmitCellEditWrite,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(effects.unwrap().is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Write is unavailable while query is running")
            );
        }

        #[test]
        fn write_rejects_stale_table_detail() {
            let mut state = editable_state();
            if let Some(detail) = state.cache.table_detail.as_mut() {
                detail.name = "posts".to_string();
            }

            let effects = reduce_query(
                &mut state,
                &Action::SubmitCellEditWrite,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(effects.unwrap().is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Table metadata does not match current preview target")
            );
        }

        #[test]
        fn submit_write_opens_confirm_dialog() {
            let mut state = editable_state();

            let effects = reduce_query(
                &mut state,
                &Action::SubmitCellEditWrite,
                Instant::now(),
                &fake_services(),
            )
            .unwrap();
            assert_eq!(effects.len(), 1);

            let dispatched = match &effects[0] {
                Effect::DispatchActions(actions) => actions.first().expect("action"),
                other => panic!("expected DispatchActions, got {:?}", other),
            };
            match dispatched {
                Action::OpenWritePreviewConfirm(preview) => {
                    assert!(preview.sql.contains("UPDATE"));
                }
                other => panic!("expected OpenWritePreviewConfirm, got {:?}", other),
            }
        }

        #[test]
        fn confirm_dialog_displays_and_executes_same_sql() {
            let mut state = editable_state();

            let effects = reduce_query(
                &mut state,
                &Action::SubmitCellEditWrite,
                Instant::now(),
                &fake_services(),
            )
            .unwrap();
            let preview = match &effects[0] {
                Effect::DispatchActions(actions) => match actions.first().expect("action") {
                    Action::OpenWritePreviewConfirm(preview) => preview.clone(),
                    other => panic!("expected OpenWritePreviewConfirm, got {:?}", other),
                },
                other => panic!("expected DispatchActions, got {:?}", other),
            };
            let expected_sql = preview.sql.clone();

            reduce_query(
                &mut state,
                &Action::OpenWritePreviewConfirm(preview),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.pending_write_preview.as_ref().map(|p| p.sql.as_str()),
                Some(expected_sql.as_str())
            );
            match &state.confirm_dialog.on_confirm {
                Action::ExecuteWrite(sql) => assert_eq!(sql, &expected_sql),
                other => panic!("expected ExecuteWrite, got {:?}", other),
            }
        }

        #[test]
        fn execute_write_success_refreshes_preview_page() {
            let mut state = editable_state();
            state.query.pagination.current_page = 2;

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 1 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(state.query.status, QueryStatus::Running);
            assert!(state.query.start_time.is_some());
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, 2 * PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 2);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn execute_write_with_non_one_row_sets_error() {
            let mut state = editable_state();

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 0 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("UPDATE expected 1 row, but affected 0 rows")
            );
        }
    }

    mod delete_write_flow {
        use super::*;
        use crate::app::write_guardrails::{
            GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
        };

        fn delete_preview() -> WritePreview {
            WritePreview {
                operation: WriteOperation::Delete,
                sql: "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '2';".to_string(),
                target_summary: TargetSummary {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    key_values: vec![("id".to_string(), "2".to_string())],
                },
                diff: vec![],
                guardrail: GuardrailDecision {
                    risk_level: RiskLevel::Low,
                    blocked: false,
                    reason: None,
                    target_summary: None,
                },
            }
        }

        #[test]
        fn open_write_preview_confirm_for_delete_sets_normal_return_mode() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::Normal;
            let preview = delete_preview();

            let effects = reduce_query(
                &mut state,
                &Action::OpenWritePreviewConfirm(Box::new(preview)),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert_eq!(state.confirm_dialog.return_mode, InputMode::Normal);
            assert_eq!(
                state.confirm_dialog.title,
                "Confirm DELETE: 1 row from users"
            );
        }

        #[test]
        fn execute_write_success_for_delete_refreshes_target_page() {
            let mut state = create_test_state();
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.pending_delete_refresh_target = Some((1, Some(499), 1));
            state.pending_write_preview = Some(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 1 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(
                state.query.post_delete_row_selection,
                PostDeleteRowSelection::Select(499)
            );
            assert_eq!(
                state.messages.last_success.as_deref(),
                Some("Deleted 1 row")
            );
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::ExecutePreview {
                    offset,
                    target_page,
                    ..
                } => {
                    assert_eq!(*offset, PREVIEW_PAGE_SIZE);
                    assert_eq!(*target_page, 1);
                }
                other => panic!("expected ExecutePreview, got {:?}", other),
            }
        }

        #[test]
        fn execute_write_non_one_rows_for_delete_sets_error() {
            let mut state = create_test_state();
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.pending_write_preview = Some(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 0 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("DELETE expected 1 row, but affected 0 rows")
            );
            assert_eq!(effects.len(), 1);
        }

        #[test]
        fn execute_write_failed_for_delete_returns_to_normal_mode() {
            let mut state = create_test_state();
            state.pending_write_preview = Some(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteFailed("boom".to_string()),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(state.messages.last_error.as_deref(), Some("boom"));
        }

        #[test]
        fn query_completed_restores_pending_row_selection() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            state.query.post_delete_row_selection = PostDeleteRowSelection::Select(1000);

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: preview_result(3),
                    generation: 1,
                    target_page: Some(0),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.result_selection.row(), Some(2));
            assert_eq!(
                state.query.post_delete_row_selection,
                PostDeleteRowSelection::Keep
            );
        }

        #[test]
        fn query_completed_clears_selection_when_requested() {
            let mut state = create_test_state();
            state.cache.selection_generation = 1;
            state.ui.result_selection.enter_row(0);
            state.query.post_delete_row_selection = PostDeleteRowSelection::Clear;

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: preview_result(2),
                    generation: 1,
                    target_page: Some(0),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.result_selection.row(), None);
            assert_eq!(
                state.query.post_delete_row_selection,
                PostDeleteRowSelection::Keep
            );
        }
    }

    mod csv_export {
        use super::*;

        fn export_test_state() -> AppState {
            let mut state = AppState::new("test_project".to_string());
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state
        }

        #[test]
        fn request_with_preview_result_emits_count_effect() {
            let mut state = export_test_state();
            state.query.current_result = Some(preview_result(10));
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.pagination.total_rows_estimate = Some(100);

            let effects = reduce_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CountRowsForExport {
                    export_query,
                    file_name,
                    ..
                } => {
                    // C1 fix: uses result.query directly instead of rebuilding SQL
                    assert_eq!(export_query, "SELECT * FROM users");
                    assert_eq!(file_name, "users");
                }
                other => panic!("expected CountRowsForExport, got {:?}", other),
            }
        }

        #[test]
        fn request_with_adhoc_result_uses_original_query() {
            let mut state = create_test_state();
            state.query.current_result = Some(adhoc_result());

            let effects = reduce_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CountRowsForExport {
                    export_query,
                    file_name,
                    ..
                } => {
                    assert_eq!(export_query, "SELECT 1");
                    assert_eq!(file_name, "adhoc");
                }
                other => panic!("expected CountRowsForExport, got {:?}", other),
            }
        }

        #[test]
        fn request_without_result_is_noop() {
            let mut state = create_test_state();
            state.query.current_result = None;

            let effects = reduce_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn rows_counted_below_threshold_emits_export_effect() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::CsvExportRowsCounted {
                    row_count: Some(500),
                    export_query: "SELECT 1".to_string(),
                    file_name: "test".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExportCsv { .. }));
        }

        #[test]
        fn rows_counted_above_threshold_opens_confirm_dialog() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::CsvExportRowsCounted {
                    row_count: Some(200_000),
                    export_query: "SELECT 1".to_string(),
                    file_name: "test".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.title.contains("CSV Export"));
        }

        #[test]
        fn rows_counted_none_opens_confirm_dialog() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::CsvExportRowsCounted {
                    row_count: None,
                    export_query: "SELECT 1".to_string(),
                    file_name: "test".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message.contains("unknown"));
        }

        #[test]
        fn export_succeeded_sets_success_message() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::CsvExportSucceeded {
                    path: "/tmp/export.csv".to_string(),
                    row_count: Some(42),
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::OpenFolder { .. }));
            assert!(
                state
                    .messages
                    .last_success
                    .as_deref()
                    .unwrap()
                    .contains("42")
            );
            assert!(
                state
                    .messages
                    .last_success
                    .as_deref()
                    .unwrap()
                    .contains("/tmp/export.csv")
            );
        }

        #[test]
        fn export_failed_sets_error_message() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::CsvExportFailed("psql error".to_string()),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.messages.last_error.as_deref(), Some("psql error"));
        }

        #[test]
        fn request_with_error_result_is_noop() {
            let mut state = create_test_state();
            state.query.current_result = Some(Arc::new(QueryResult::error(
                "SELECT 1".to_string(),
                "error".to_string(),
                10,
                QuerySource::Adhoc,
            )));

            let effects = reduce_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }
    }

    mod adhoc_refresh {
        use super::*;

        #[test]
        fn dml_with_table_selected_emits_execute_preview() {
            let mut state = state_with_table("public", "users");

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Update(3)),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::ExecutePreview { table, .. } if table == "users")
            );
        }

        #[test]
        fn dml_without_table_selected_emits_no_effects() {
            let mut state = create_test_state();

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Insert(1)),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn ddl_emits_cache_invalidate_and_fetch_metadata() {
            let mut state = state_with_table("public", "users");

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::CacheInvalidate { .. }))
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { .. }))
            );
        }

        #[test]
        fn ddl_resets_prefetch_state_and_clears_table_detail() {
            let mut state = state_with_table("public", "users");
            state.sql_modal.prefetch_started = true;
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.users".to_string());
            state.cache.table_detail = Some(users_table_detail());

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(!state.sql_modal.prefetch_started);
            assert!(state.sql_modal.prefetch_queue.is_empty());
            assert!(state.cache.table_detail.is_none());
        }

        #[test]
        fn tcl_emits_no_effects() {
            for tag in [CommandTag::Begin, CommandTag::Commit, CommandTag::Rollback] {
                let mut state = state_with_table("public", "users");

                let effects = reduce_query(
                    &mut state,
                    &Action::QueryCompleted {
                        result: adhoc_result_with_tag(tag),
                        generation: 0,
                        target_page: None,
                    },
                    Instant::now(),
                    &AppServices::stub(),
                )
                .unwrap();

                assert!(effects.is_empty());
            }
        }

        #[test]
        fn select_emits_no_effects() {
            let mut state = state_with_table("public", "users");

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Select(5)),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn adhoc_error_emits_no_effects() {
            let mut state = state_with_table("public", "users");

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_error_result(),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn no_command_tag_emits_no_effects() {
            let mut state = state_with_table("public", "users");
            let result = Arc::new(QueryResult {
                query: "SELECT 1".to_string(),
                columns: vec!["?column?".to_string()],
                rows: vec![vec!["1".to_string()]],
                row_count: 1,
                execution_time_ms: 5,
                executed_at: Instant::now(),
                source: QuerySource::Adhoc,
                error: None,
                command_tag: None,
            });

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }
    }

    mod adhoc_refresh_integration {
        use super::*;
        use crate::app::reducers::metadata::reduce_metadata;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(tables: Vec<(&str, &str)>) -> Box<DatabaseMetadata> {
            Box::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None, false)
                    })
                    .collect(),
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn dml_then_preview_updates_current_result() {
            let mut state = state_with_table("public", "users");

            // Step 1: DML → ExecutePreview effect
            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Update(3)),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecutePreview { .. }));

            // Step 2: simulated preview response → current_result updated
            let new_preview = preview_result(5);
            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: Arc::clone(&new_preview),
                    generation: 0,
                    target_page: Some(0),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            let stored = state.query.current_result.as_ref().unwrap();
            assert_eq!(stored.source, QuerySource::Preview);
            assert_eq!(stored.row_count, 5);
        }

        #[test]
        fn ddl_create_then_metadata_loaded_preserves_explorer_selection() {
            let mut state = state_with_table("public", "users");

            // Step 1: DDL CREATE → metadata effects
            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(!state.sql_modal.prefetch_started);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            // Step 2: MetadataLoaded with "users" still present → selection preserved + preview refreshed
            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let meta_effects = reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            )
            .unwrap();

            // "users" is at index 1 (alphabetical: orders=0, users=1)
            assert_eq!(state.ui.explorer_selected, 1);
            assert_eq!(state.query.pagination.table, "users");
            assert!(
                meta_effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { table, .. } if table == "users"))
            );
        }

        #[test]
        fn ddl_drop_then_metadata_loaded_without_table_clears_selection() {
            let mut state = state_with_table("public", "users");
            state.query.current_result = Some(preview_result(3));

            // Step 1: DROP TABLE
            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            // Step 2: MetadataLoaded without "users" → selection cleared
            let metadata = make_metadata(vec![("public", "orders")]);
            reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            );

            assert!(state.query.pagination.table.is_empty());
            assert!(state.query.current_result.is_none());
            assert!(state.cache.table_detail.is_none());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn ddl_does_not_emit_execute_preview_so_modal_status_stays_success() {
            let mut state = state_with_table("public", "users");

            let effects = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { .. }))
            );
            assert_eq!(
                state.sql_modal.status,
                crate::app::sql_modal_context::SqlModalStatus::Success
            );
        }

        #[test]
        fn success_snapshot_not_overwritten_by_subsequent_preview_result() {
            let mut state = state_with_table("public", "users");

            // Step 1: DDL → last_adhoc_success is set
            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: adhoc_result_with_tag(CommandTag::Alter("TABLE".to_string())),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            let saved_tag = state
                .sql_modal
                .last_adhoc_success
                .as_ref()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(saved_tag, Some(CommandTag::Alter(_))));

            // Step 2: preview result arrives (simulating MetadataLoaded → ExecutePreview)
            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: preview_result(5),
                    generation: 0,
                    target_page: Some(0),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            // last_adhoc_success must still hold the DDL result — not cleared by preview
            let tag_after = state
                .sql_modal
                .last_adhoc_success
                .as_ref()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(tag_after, Some(CommandTag::Alter(_))));
        }
    }
}
