use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::{DatabaseType, QuerySource, QueryValue};
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::{ConfirmIntent, CsvExportCacheSnapshot};
use crate::model::shared::input_mode::InputMode;
use crate::policy::sql::sqlite_export::{SqliteExportPlan, sqlite_export_plan};
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::browse::query::preview_effect_for_current_table;
use crate::update::dispatch_result::DispatchResult;

const LARGE_EXPORT_THRESHOLD: usize = 100_000;

fn csv_export_file_name(state: &AppState, source: QuerySource) -> String {
    match source {
        QuerySource::Preview => {
            let table = state.query.pagination.table();
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
    }
}

fn dispatch_cached_csv_export(
    state: &mut AppState,
    dsn: String,
    run_id: u64,
    file_name: String,
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
    row_count: Option<usize>,
) -> DispatchResult {
    let needs_confirm = row_count.is_some_and(|count| count > LARGE_EXPORT_THRESHOLD);
    if needs_confirm {
        let msg = match row_count {
            Some(n) => format!("Export {n} rows to CSV? This may take a while."),
            None => "Export to CSV?".to_string(),
        };
        state.confirm_dialog.open(
            "Confirm CSV Export",
            msg,
            ConfirmIntent::CsvExportCached {
                dsn,
                run_id,
                file_name,
                row_count,
                snapshot: CsvExportCacheSnapshot { columns, values },
            },
        );
        state.modal.push_mode(InputMode::ConfirmDialog);
        DispatchResult::handled()
    } else {
        DispatchResult::handled_with(vec![Effect::ExportCsvFromCache {
            dsn,
            run_id,
            file_name,
            columns,
            values,
            row_count,
        }])
    }
}

fn dispatch_rerunnable_csv_export(
    state: &AppState,
    dsn: String,
    run_id: u64,
    export_query: String,
    file_name: String,
) -> DispatchResult {
    let stripped = export_query.trim_end().trim_end_matches(';').to_string();
    let count_query = format!("SELECT COUNT(*) FROM ({stripped}) AS _export_count");
    DispatchResult::handled_with(vec![Effect::CountRowsForExport {
        dsn,
        run_id,
        count_query,
        export_query,
        file_name,
        read_only: state.session.is_read_only(),
    }])
}

pub fn reduce_pagination(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::RequestCsvExport => {
            if !state.can_request_csv_export() {
                return DispatchResult::handled();
            }
            let Some(result) = state.query.visible_result() else {
                return DispatchResult::handled();
            };
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };

            let export_query = result.query.clone();
            let file_name = csv_export_file_name(state, result.source);
            let row_count = result.row_count();

            if state.session.active_database_type() == Some(DatabaseType::SQLite) {
                match sqlite_export_plan(result.source, &export_query, &result.columns, row_count) {
                    SqliteExportPlan::NotExportable { reason } => {
                        state.messages.set_error_at(reason, now);
                        return DispatchResult::handled();
                    }
                    SqliteExportPlan::RerunnableQuery { query } => {
                        let run_id = state.query.begin_running(now);
                        return dispatch_rerunnable_csv_export(
                            state, dsn, run_id, query, file_name,
                        );
                    }
                    SqliteExportPlan::CachedResult { row_count } => {
                        let columns = result.columns.clone();
                        let values = result.values().to_vec();
                        let run_id = state.query.begin_running(now);
                        return dispatch_cached_csv_export(
                            state,
                            dsn,
                            run_id,
                            file_name,
                            columns,
                            values,
                            Some(row_count),
                        );
                    }
                }
            }

            let run_id = state.query.begin_running(now);
            dispatch_rerunnable_csv_export(state, dsn, run_id, export_query, file_name)
        }

        Action::CsvExportRowsCounted {
            dsn,
            run_id,
            row_count,
            export_query,
            file_name,
        } => {
            if state.is_stale_query_run(dsn, *run_id) {
                return DispatchResult::handled();
            }

            let needs_confirm = match row_count {
                Some(n) => *n > LARGE_EXPORT_THRESHOLD,
                None => true,
            };

            if needs_confirm {
                let msg = match row_count {
                    Some(n) => format!("Export {n} rows to CSV? This may take a while."),
                    None => "Row count unknown. Export to CSV?".to_string(),
                };
                state.confirm_dialog.open(
                    "Confirm CSV Export",
                    msg,
                    ConfirmIntent::CsvExportRerunnable {
                        dsn: dsn.clone(),
                        run_id: *run_id,
                        export_query: export_query.clone(),
                        file_name: file_name.clone(),
                        row_count: *row_count,
                    },
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
                DispatchResult::handled()
            } else {
                DispatchResult::handled_with(vec![Effect::ExportCsv {
                    dsn: dsn.clone(),
                    run_id: *run_id,
                    query: export_query.clone(),
                    file_name: file_name.clone(),
                    row_count: *row_count,
                    read_only: state.session.is_read_only(),
                }])
            }
        }

        Action::ExecuteCsvExport {
            dsn,
            run_id,
            export_query,
            file_name,
            row_count,
        } => {
            if state.is_stale_query_run(dsn, *run_id) {
                return DispatchResult::handled();
            }

            DispatchResult::handled_with(vec![Effect::ExportCsv {
                dsn: dsn.clone(),
                run_id: *run_id,
                query: export_query.clone(),
                file_name: file_name.clone(),
                row_count: *row_count,
                read_only: state.session.is_read_only(),
            }])
        }

        Action::CsvExportSucceeded {
            dsn,
            run_id,
            path,
            row_count,
        } => {
            if state.is_stale_query_run(dsn, *run_id) {
                return DispatchResult::handled();
            }

            state.query.mark_idle();
            let msg = match row_count {
                Some(n) => format!("Exported {n} rows → {path}"),
                None => format!("Exported → {path}"),
            };
            state.messages.set_success_at(msg, now);
            let folder = Path::new(path)
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            DispatchResult::handled_with(vec![Effect::OpenFolder { path: folder }])
        }

        Action::CsvExportFailed { dsn, run_id, error } => {
            if state.is_stale_query_run(dsn, *run_id) {
                return DispatchResult::handled();
            }

            state.query.mark_idle();
            state.messages.set_error_at(error.user_message(), now);
            DispatchResult::handled()
        }

        Action::OpenFolderFailed(error) => {
            state
                .messages
                .set_error_at(format!("Failed to open folder: {error}"), now);

            DispatchResult::handled()
        }

        Action::ResultNextPage => {
            if state.query.is_running() || !state.query.can_paginate_visible_result() {
                return DispatchResult::handled();
            }
            if !state.query.pagination.can_next() {
                return DispatchResult::handled();
            }
            let next_page = state.query.pagination.next_page();
            let generation = state.session.selection_generation();
            match preview_effect_for_current_table(state, now, next_page, generation) {
                Some(effect) => {
                    state.result_interaction.reset_view();
                    DispatchResult::handled_with(vec![effect])
                }
                None => DispatchResult::handled(),
            }
        }

        Action::ResultPrevPage => {
            if state.query.is_running() || !state.query.can_paginate_visible_result() {
                return DispatchResult::handled();
            }
            if !state.query.pagination.can_prev() {
                return DispatchResult::handled();
            }
            let prev_page = state.query.pagination.prev_page();
            let generation = state.session.selection_generation();
            match preview_effect_for_current_table(state, now, prev_page, generation) {
                Some(effect) => {
                    state.result_interaction.reset_view();
                    state.query.pagination.clear_reached_end();
                    DispatchResult::handled_with(vec![effect])
                }
                None => DispatchResult::handled(),
            }
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{QueryResult, QuerySource};
    use crate::ports::outbound::DbOperationError;
    use crate::update::test_fixtures;
    use std::sync::Arc;

    use crate::model::browse::query_execution::PREVIEW_PAGE_SIZE;
    use crate::update::browse::query::dispatch_query;
    use crate::update::browse::query::tests::*;

    fn csv_rows_counted_action(
        state: &mut AppState,
        row_count: Option<usize>,
        export_query: &str,
        file_name: &str,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExportRowsCounted {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            row_count,
            export_query: export_query.to_string(),
            file_name: file_name.to_string(),
        }
    }

    fn csv_succeeded_action(state: &mut AppState, path: &str, row_count: Option<usize>) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExportSucceeded {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            path: path.to_string(),
            row_count,
        }
    }

    fn csv_failed_action(state: &mut AppState, error: DbOperationError) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExportFailed {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            error,
        }
    }

    fn preview_result_with_two_columns(row_count: usize) -> Arc<QueryResult> {
        let rows: Vec<Vec<String>> = (0..row_count)
            .map(|i| vec![i.to_string(), format!("name_{i}")])
            .collect();
        Arc::new(QueryResult::success(
            "SELECT * FROM users".to_string(),
            vec!["id".to_string(), "name".to_string()],
            rows,
            10,
            QuerySource::Preview,
        ))
    }

    mod next_page {
        use super::*;

        #[test]
        fn emits_correct_offset_for_next_page() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state
                .query
                .pagination
                .reset_for_table_with_estimate("public", "users", Some(1500));
            let now = Instant::now();

            let effects = dispatch_query(
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
                other => panic!("expected ExecutePreview, got {other:?}"),
            }
        }

        #[test]
        fn noop_when_reached_end() {
            let mut state = create_test_state();
            state.query.set_current_result(preview_result(100));
            state.query.pagination.set_page_result(0, true);
            let now = Instant::now();

            let effects = dispatch_query(
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
            state.query.set_current_result(adhoc_result());
            let now = Instant::now();

            let effects = dispatch_query(
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
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            let _ = state.query.begin_running(Instant::now());
            let now = Instant::now();

            let effects = dispatch_query(
                &mut state,
                &Action::ResultNextPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn preserves_view_state_when_next_page_noops() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(100));
            state.query.pagination.set_page_result(0, true);
            state.result_interaction.activate_cell(2, 1);
            state.result_interaction.stage_row(2);

            dispatch_query(
                &mut state,
                &Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.result_interaction.selection().row(), Some(2));
            assert_eq!(state.result_interaction.selection().cell(), Some(1));
            assert!(state.result_interaction.staged_delete_rows().contains(&2));
        }

        #[test]
        fn transition_resets_view_state() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state
                .query
                .pagination
                .reset_for_table_with_estimate("public", "users", Some(1500));
            state.result_interaction.activate_cell(3, 1);
            state.result_interaction.stage_row(3);

            dispatch_query(
                &mut state,
                &Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.result_interaction.selection().row().is_none());
            assert!(state.result_interaction.selection().cell().is_none());
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }
    }

    mod prev_page {
        use super::*;

        #[test]
        fn emits_correct_offset_for_prev_page() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state
                .query
                .pagination
                .reset_for_table_with_estimate("public", "users", Some(1500));
            state.query.pagination.set_page_result(2, false);
            let now = Instant::now();

            let effects = dispatch_query(
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
                other => panic!("expected ExecutePreview, got {other:?}"),
            }
        }

        #[test]
        fn noop_on_first_page() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination.set_current_page(0);
            let now = Instant::now();

            let effects = dispatch_query(
                &mut state,
                &Action::ResultPrevPage,
                now,
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn preserves_view_state_when_prev_page_noops() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(PREVIEW_PAGE_SIZE));
            state.query.pagination.set_current_page(0);
            state.result_interaction.activate_cell(1, 1);
            state.result_interaction.stage_row(1);

            dispatch_query(
                &mut state,
                &Action::ResultPrevPage,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.result_interaction.selection().row(), Some(1));
            assert_eq!(state.result_interaction.selection().cell(), Some(1));
            assert!(state.result_interaction.staged_delete_rows().contains(&1));
        }
    }

    mod csv_export {
        use super::*;
        use crate::domain::QueryResult;
        use crate::ports::outbound::DbOperationError;
        use rstest::rstest;

        fn export_test_state() -> AppState {
            let mut state = AppState::new("test_project".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
            state
        }

        #[test]
        fn request_with_preview_result_emits_count_effect() {
            let mut state = export_test_state();
            state.query.set_current_result(preview_result(10));
            state.query.pagination.reset_for_table("public", "users");
            state.query.pagination.set_total_rows_estimate(Some(100));

            let effects = dispatch_query(
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
                    assert_eq!(export_query, "SELECT * FROM users");
                    assert_eq!(file_name, "users");
                }
                other => panic!("expected CountRowsForExport, got {other:?}"),
            }
        }

        #[test]
        fn request_with_adhoc_result_uses_original_query() {
            let mut state = create_test_state();
            state.query.set_current_result(adhoc_result());

            let effects = dispatch_query(
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
                other => panic!("expected CountRowsForExport, got {other:?}"),
            }
        }

        #[rstest]
        #[case::insert("INSERT INTO users(name) VALUES ('a') RETURNING id")]
        #[case::update("UPDATE users SET name = 'b' WHERE id = 1 RETURNING id")]
        #[case::delete("DELETE FROM users WHERE id = 1 RETURNING id")]
        fn request_with_mutating_returning_result_is_noop(#[case] query: &str) {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    query.to_string(),
                    vec!["id".to_string()],
                    vec![vec!["1".to_string()]],
                    10,
                    QuerySource::Adhoc,
                )));

            let effects = dispatch_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
        }

        #[test]
        fn request_without_result_is_noop() {
            let mut state = create_test_state();
            state.query.clear_current_result();

            let effects = dispatch_query(
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
            let action = csv_rows_counted_action(&mut state, Some(500), "SELECT 1", "test");

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExportCsv { .. }));
        }

        #[test]
        fn rows_counted_above_threshold_opens_confirm_dialog() {
            let mut state = create_test_state();
            let action = csv_rows_counted_action(&mut state, Some(200_000), "SELECT 1", "test");

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.title().contains("CSV Export"));
        }

        #[test]
        fn rows_counted_none_opens_confirm_dialog() {
            let mut state = create_test_state();
            let action = csv_rows_counted_action(&mut state, None, "SELECT 1", "test");

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
        }

        #[test]
        fn export_succeeded_sets_success_message() {
            let mut state = create_test_state();
            let action = csv_succeeded_action(&mut state, "/tmp/export.csv", Some(42));

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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
            let action = csv_failed_action(
                &mut state,
                DbOperationError::QueryFailed("psql error".to_string()),
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Query failed: psql error. Review the database error details and SQL.")
            );
        }

        #[test]
        fn stale_rows_counted_does_not_open_confirm_or_export() {
            let mut state = create_test_state();
            let old_run_id = begin_query_run(&mut state);
            let _ = begin_query_run(&mut state);

            let effects = dispatch_query(
                &mut state,
                &Action::CsvExportRowsCounted {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: old_run_id,
                    row_count: Some(200_000),
                    export_query: "SELECT 1".to_string(),
                    file_name: "test".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_ne!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.query.is_running());
        }

        #[test]
        fn request_with_error_result_is_noop() {
            let mut state = create_test_state();
            state.query.set_current_result(Arc::new(QueryResult::error(
                "SELECT 1".to_string(),
                "error".to_string(),
                10,
                QuerySource::Adhoc,
            )));

            let effects = dispatch_query(
                &mut state,
                &Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        mod sqlite {
            use super::*;

            fn sqlite_state() -> AppState {
                let mut state = AppState::new("test_project".to_string());
                test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/test.db");
                state
            }

            #[test]
            fn write_only_query_shows_not_exportable_error() {
                let mut state = sqlite_state();
                state
                    .query
                    .set_current_result(Arc::new(QueryResult::success(
                        "INSERT INTO users(id) VALUES (1)".to_string(),
                        vec![],
                        vec![],
                        1,
                        QuerySource::Adhoc,
                    )));

                let effects = dispatch_query(
                    &mut state,
                    &Action::RequestCsvExport,
                    Instant::now(),
                    &AppServices::stub(),
                )
                .unwrap();

                assert!(effects.is_empty());
                assert!(
                    state
                        .messages
                        .last_error
                        .as_deref()
                        .unwrap()
                        .contains("Cannot export")
                );
            }

            #[test]
            fn mixed_query_exports_cached_rows_without_count_effect() {
                let mut state = sqlite_state();
                state
                    .query
                    .set_current_result(Arc::new(QueryResult::success(
                        "INSERT INTO users(id) VALUES (1); SELECT id FROM users".to_string(),
                        vec!["id".to_string()],
                        vec![vec!["1".to_string()]],
                        1,
                        QuerySource::Adhoc,
                    )));

                let effects = dispatch_query(
                    &mut state,
                    &Action::RequestCsvExport,
                    Instant::now(),
                    &AppServices::stub(),
                )
                .unwrap();

                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::ExportCsvFromCache { .. }));
            }

            #[test]
            fn select_still_uses_count_effect() {
                let mut state = sqlite_state();
                state
                    .query
                    .set_current_result(Arc::new(QueryResult::success(
                        "SELECT id FROM users".to_string(),
                        vec!["id".to_string()],
                        vec![vec!["1".to_string()]],
                        1,
                        QuerySource::Adhoc,
                    )));

                let effects = dispatch_query(
                    &mut state,
                    &Action::RequestCsvExport,
                    Instant::now(),
                    &AppServices::stub(),
                )
                .unwrap();

                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::CountRowsForExport { .. }));
            }

            #[test]
            fn preview_exports_visible_typed_values_from_cache() {
                let mut state = sqlite_state();
                state.query.set_current_result(Arc::new(
                    QueryResult::success_with_values(
                        "SELECT \"_rowid_\" AS \"__sabiql_rowid\", CASE WHEN typeof(\"message\") = 'text' THEN hex(\"message\") END AS \"message\" FROM \"logs\"".to_string(),
                        vec!["message".to_string()],
                        vec![vec![QueryValue::text("a\0bc")]],
                        1,
                        QuerySource::Preview,
                    ),
                ));

                let effects = dispatch_query(
                    &mut state,
                    &Action::RequestCsvExport,
                    Instant::now(),
                    &AppServices::stub(),
                )
                .unwrap();

                let Effect::ExportCsvFromCache {
                    columns, values, ..
                } = &effects[0]
                else {
                    panic!("expected cached CSV export effect");
                };
                assert_eq!(columns, &["message"]);
                assert_eq!(values, &vec![vec![QueryValue::text("a\0bc")]]);
            }
        }
    }
}
