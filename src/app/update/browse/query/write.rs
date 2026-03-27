use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
use crate::app::model::shared::input_mode::InputMode;
use crate::app::policy::write::write_guardrails::{
    ColumnDiff, RiskLevel, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::policy::write::write_update::{build_pk_pairs, escape_preview_value};
use crate::app::services::AppServices;
use crate::app::update::action::Action;
use crate::app::update::helpers::{build_bulk_delete_preview, editable_preview_base};

fn build_update_preview(state: &AppState, services: &AppServices) -> Result<WritePreview, String> {
    if !state.result_interaction.cell_edit().is_active() {
        return Err("No active cell edit session".to_string());
    }

    let (result, pk_cols) = editable_preview_base(state)?;

    let row_idx = state
        .result_interaction
        .cell_edit()
        .row
        .ok_or_else(|| "No row selected for edit".to_string())?;
    let col_idx = state
        .result_interaction
        .cell_edit()
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
    let target = crate::app::policy::write::write_guardrails::TargetSummary {
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
            .unwrap_or_else(|| "Write blocked by guardrails".to_string());
        return Err(reason);
    }

    let sql = services.sql_dialect.build_update_sql(
        &target.schema,
        &target.table,
        &column_name,
        state.result_interaction.cell_edit().draft_value(),
        &target.key_values,
    );
    let preview = WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: target,
        diff: vec![ColumnDiff {
            column: column_name,
            before: state.result_interaction.cell_edit().original_value.clone(),
            after: state
                .result_interaction
                .cell_edit()
                .draft_value()
                .to_string(),
        }],
        guardrail,
    };
    Ok(preview)
}

fn build_write_preview_fallback_message(preview: &WritePreview) -> String {
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

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::SubmitCellEditWrite => {
            if !state.result_interaction.staged_delete_rows().is_empty() {
                match build_bulk_delete_preview(state, services) {
                    Ok((preview, target_page, target_row)) => {
                        let staged_count = state.result_interaction.staged_delete_rows().len();
                        state.query.set_delete_refresh_target(
                            target_page,
                            target_row,
                            staged_count,
                        );
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

            if !state.result_interaction.cell_edit().is_active() {
                state
                    .messages
                    .set_error_at("No active cell edit session".to_string(), now);
                return Some(vec![]);
            }
            if state.query.is_running() {
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
            if state.session.read_only {
                state.messages.set_error_at(
                    "Read-only mode: write operations are disabled".to_string(),
                    now,
                );
                return Some(vec![]);
            }
            state
                .result_interaction
                .set_write_preview((**preview).clone());
            let operation = preview.operation;
            let title = match operation {
                WriteOperation::Update => {
                    state.query.clear_delete_refresh_target();
                    format!("Confirm UPDATE: {}", preview.target_summary.table)
                }
                WriteOperation::Delete => {
                    let n = state
                        .query
                        .pending_delete_refresh_target()
                        .map_or(1, |(_, _, count)| count);
                    format!(
                        "Confirm DELETE: {} {} from {}",
                        n,
                        if n == 1 { "row" } else { "rows" },
                        preview.target_summary.table
                    )
                }
            };

            state.confirm_dialog.open(
                title,
                build_write_preview_fallback_message(preview),
                crate::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
                    sql: preview.sql.clone(),
                    blocked: preview.guardrail.blocked,
                },
            );
            if matches!(operation, WriteOperation::Delete) {
                state.modal.set_mode(InputMode::Normal);
            }
            state.modal.push_mode(InputMode::ConfirmDialog);

            Some(vec![])
        }

        Action::ExecuteWrite(query) => {
            if state.session.read_only {
                state.messages.set_error_at(
                    "Read-only mode: write operations are disabled".to_string(),
                    now,
                );
                return Some(vec![]);
            }
            if let Some(dsn) = &state.session.dsn {
                state.query.begin_running(now);
                Some(vec![Effect::ExecuteWrite {
                    dsn: dsn.clone(),
                    query: query.clone(),
                    read_only: state.session.read_only,
                }])
            } else {
                state
                    .messages
                    .set_error_at("No active connection".to_string(), now);
                Some(vec![])
            }
        }

        Action::ExecuteWriteSucceeded { affected_rows } => {
            state.query.mark_idle();
            let operation = state
                .result_interaction
                .pending_write_preview()
                .map_or(WriteOperation::Update, |p| p.operation);
            state.result_interaction.clear_write_preview();
            match operation {
                WriteOperation::Update => {
                    if *affected_rows != 1 {
                        state.messages.set_error_at(
                            format!("UPDATE expected 1 row, but affected {affected_rows} rows"),
                            now,
                        );
                        state.modal.set_mode(InputMode::CellEdit);
                        return Some(vec![]);
                    }

                    state
                        .messages
                        .set_success_at("Updated 1 row".to_string(), now);
                    state.result_interaction.clear_cell_edit();
                    state.modal.set_mode(InputMode::Normal);

                    if let Some(dsn) = &state.session.dsn {
                        let page = state.query.pagination.current_page;
                        state.query.begin_running(now);
                        Some(vec![Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema: state.query.pagination.schema.clone(),
                            table: state.query.pagination.table.clone(),
                            generation: state.session.selection_generation(),
                            limit: PREVIEW_PAGE_SIZE,
                            offset: page * PREVIEW_PAGE_SIZE,
                            target_page: page,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
                WriteOperation::Delete => {
                    let (target_page, target_row, expected) = state
                        .query
                        .take_delete_refresh_target()
                        .unwrap_or((state.query.pagination.current_page, None, 1));

                    let row_word = |n: usize| if n == 1 { "row" } else { "rows" };
                    if *affected_rows == expected {
                        state.messages.set_success_at(
                            format!("Deleted {} {}", expected, row_word(expected)),
                            now,
                        );
                    } else {
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
                    }
                    state.result_interaction.clear_cell_edit();
                    state.result_interaction.clear_staged_deletes();
                    state.modal.set_mode(InputMode::Normal);

                    state.query.set_post_delete_selection(target_row.map_or(
                        PostDeleteRowSelection::Clear,
                        PostDeleteRowSelection::Select,
                    ));

                    if let Some(dsn) = &state.session.dsn {
                        state.query.begin_running(now);
                        state.query.pagination.reached_end = false;
                        Some(vec![Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema: state.query.pagination.schema.clone(),
                            table: state.query.pagination.table.clone(),
                            generation: state.session.selection_generation(),
                            limit: PREVIEW_PAGE_SIZE,
                            offset: target_page * PREVIEW_PAGE_SIZE,
                            target_page,
                            read_only: state.session.read_only,
                        }])
                    } else {
                        Some(vec![])
                    }
                }
            }
        }

        Action::ExecuteWriteFailed(error) => {
            state.query.mark_idle();
            let operation = state
                .result_interaction
                .pending_write_preview()
                .map_or(WriteOperation::Update, |p| p.operation);
            state.result_interaction.clear_write_preview();
            state.query.clear_delete_refresh_target();
            state.messages.set_error_at(error.to_string(), now);
            state.modal.set_mode(match operation {
                WriteOperation::Update => InputMode::CellEdit,
                WriteOperation::Delete => InputMode::Normal,
            });
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::browse::query_execution::{PostDeleteRowSelection, QueryStatus};
    use crate::app::policy::write::write_guardrails::{
        GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
    };
    use crate::app::update::browse::query::reduce_query;
    use crate::app::update::browse::query::tests::*;

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
                let set_clause = format!("\"{column}\" = '{new_value}'");
                let where_clause: Vec<String> = pk_pairs
                    .iter()
                    .map(|(k, v)| format!("\"{k}\" = '{v}'"))
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
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.query.set_current_result(editable_preview_result());
            state
                .session
                .set_table_detail_raw(Some(users_table_detail()));
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.modal.set_mode(InputMode::CellEdit);
            state
                .result_interaction
                .begin_cell_edit(0, 1, "Alice".to_string());
            state
                .result_interaction
                .cell_edit_input_mut()
                .set_content("Bob".to_string());
            state
        }

        #[test]
        fn write_requires_cell_edit_mode() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::Normal);

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
            state.query.begin_running(Instant::now());

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
            if let Some(mut detail) = state.session.table_detail().cloned() {
                detail.name = "posts".to_string();
                state.session.set_table_detail_raw(Some(detail));
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
                other => panic!("expected DispatchActions, got {other:?}"),
            };
            match dispatched {
                Action::OpenWritePreviewConfirm(preview) => {
                    assert!(preview.sql.contains("UPDATE"));
                }
                other => panic!("expected OpenWritePreviewConfirm, got {other:?}"),
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
                    other => panic!("expected OpenWritePreviewConfirm, got {other:?}"),
                },
                other => panic!("expected DispatchActions, got {other:?}"),
            };
            let expected_sql = preview.sql.clone();

            reduce_query(
                &mut state,
                &Action::OpenWritePreviewConfirm(preview),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state
                    .result_interaction
                    .pending_write_preview()
                    .map(|p| p.sql.as_str()),
                Some(expected_sql.as_str())
            );
            match state.confirm_dialog.intent() {
                Some(crate::app::model::shared::confirm_dialog::ConfirmIntent::ExecuteWrite {
                    sql,
                    blocked,
                }) => {
                    assert_eq!(sql, &expected_sql);
                    assert!(!blocked);
                }
                other => panic!("expected ExecuteWrite intent, got {other:?}"),
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

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(state.query.status(), QueryStatus::Running);
            assert!(state.query.start_time().is_some());
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
                other => panic!("expected ExecutePreview, got {other:?}"),
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
            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("UPDATE expected 1 row, but affected 0 rows")
            );
        }
    }

    mod delete_write_flow {
        use super::*;
        use crate::app::ports::DbOperationError;

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
            state.modal.set_mode(InputMode::Normal);
            let preview = delete_preview();

            let effects = reduce_query(
                &mut state,
                &Action::OpenWritePreviewConfirm(Box::new(preview)),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert_eq!(state.modal.return_destination(), InputMode::Normal);
            assert_eq!(
                state.confirm_dialog.title(),
                "Confirm DELETE: 1 row from users"
            );
        }

        #[test]
        fn execute_write_success_for_delete_refreshes_target_page() {
            let mut state = create_test_state();
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.set_delete_refresh_target(1, Some(499), 1);
            state.result_interaction.set_write_preview(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 1 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.query.post_delete_row_selection(),
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
                other => panic!("expected ExecutePreview, got {other:?}"),
            }
        }

        #[test]
        fn execute_write_non_one_rows_for_delete_sets_error() {
            let mut state = create_test_state();
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.result_interaction.set_write_preview(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteSucceeded { affected_rows: 0 },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("DELETE expected 1 row, but affected 0 rows")
            );
            assert_eq!(effects.len(), 1);
        }

        #[test]
        fn execute_write_failed_for_delete_returns_to_normal_mode() {
            let mut state = create_test_state();
            state.result_interaction.set_write_preview(delete_preview());

            let effects = reduce_query(
                &mut state,
                &Action::ExecuteWriteFailed(DbOperationError::QueryFailed("boom".to_string())),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Query failed: boom")
            );
        }

        #[test]
        fn query_completed_restores_pending_row_selection() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Select(1000));

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

            assert_eq!(state.result_interaction.selection().row(), Some(2));
            assert_eq!(
                state.query.post_delete_row_selection(),
                PostDeleteRowSelection::Keep
            );
        }

        #[test]
        fn query_completed_clears_selection_when_requested() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state.result_interaction.enter_row(0);
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Clear);

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

            assert_eq!(state.result_interaction.selection().row(), None);
            assert_eq!(
                state.query.post_delete_row_selection(),
                PostDeleteRowSelection::Keep
            );
        }
    }
}
