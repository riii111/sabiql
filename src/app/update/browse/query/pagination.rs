use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::{DatabaseType, QuerySource, QueryValue, mysql_sql::mysql_export_plan};
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::{ConfirmIntent, CsvExportCacheSnapshot};
use crate::model::shared::input_mode::InputMode;
use crate::policy::sql::sqlite_export::{SqliteExportPlan, sqlite_export_plan};
use crate::update::action::Action;
use crate::update::browse::query::preview_effect_for_current_table;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

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
    row_count: usize,
) -> DispatchResult {
    let needs_confirm = row_count > LARGE_EXPORT_THRESHOLD;
    if needs_confirm {
        let msg = format!("Export {row_count} rows to CSV? This may take a while.");
        state.confirm_dialog.open(
            "Confirm CSV Export",
            msg,
            ConfirmIntent::CsvExportCached {
                dsn,
                run_id,
                file_name,
                row_count: Some(row_count),
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
            row_count: Some(row_count),
        }])
    }
}

fn dispatch_rerunnable_csv_export(
    state: &mut AppState,
    dsn: String,
    run_id: u64,
    export_query: String,
    file_name: String,
) -> DispatchResult {
    state.confirm_dialog.open(
        "Confirm CSV Export",
        "Row count unknown. Export to CSV?",
        ConfirmIntent::CsvExportRerunnable {
            dsn,
            run_id,
            export_query,
            file_name,
        },
    );
    state.modal.push_mode(InputMode::ConfirmDialog);
    DispatchResult::handled()
}

pub(in crate::update) fn reduce_pagination(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::RequestCsvExport => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if !state.can_request_csv_export() {
                return DispatchResult::handled();
            }
            let Some(result) = state.query.visible_result() else {
                return DispatchResult::handled();
            };
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };

            let file_name = csv_export_file_name(state, result.source);
            let row_count = result.row_count();

            if state.session.active_database_type() == Some(DatabaseType::SQLite) {
                match sqlite_export_plan(result.source, &result.query, &result.columns, row_count) {
                    SqliteExportPlan::NotExportable { reason } => {
                        state.messages.set_error(reason);
                        return DispatchResult::handled();
                    }
                    SqliteExportPlan::RerunnableQuery { query } => {
                        let run_id = state.query.begin_non_preview_running(now);
                        return dispatch_rerunnable_csv_export(
                            state, dsn, run_id, query, file_name,
                        );
                    }
                    SqliteExportPlan::CachedResult { row_count } => {
                        let columns = result.columns.clone();
                        let values = result.values().to_vec();
                        let run_id = state.query.begin_non_preview_running(now);
                        return dispatch_cached_csv_export(
                            state, dsn, run_id, file_name, columns, values, row_count,
                        );
                    }
                }
            }

            if state.session.active_database_type() == Some(DatabaseType::MySQL) {
                if result.source == QuerySource::Preview {
                    let columns = result.columns.clone();
                    let values = result.values().to_vec();
                    let run_id = state.query.begin_non_preview_running(now);
                    return dispatch_cached_csv_export(
                        state, dsn, run_id, file_name, columns, values, row_count,
                    );
                }

                if mysql_export_plan(&result.query).is_none() {
                    return DispatchResult::handled();
                }
                let export_query = result.query.clone();
                let run_id = state.query.begin_non_preview_running(now);
                return dispatch_rerunnable_csv_export(state, dsn, run_id, export_query, file_name);
            }

            let export_query = result.query.clone();
            let run_id = state.query.begin_non_preview_running(now);
            dispatch_rerunnable_csv_export(state, dsn, run_id, export_query, file_name)
        }

        Action::CsvExportSucceeded {
            run_id,
            path,
            row_count,
        } => {
            if !state.query.is_current_run(*run_id) {
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

        Action::CsvExportFailed { run_id, error } => {
            if !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            state.query.mark_idle();
            state.messages.set_error(error.user_message());
            DispatchResult::handled()
        }

        Action::OpenFolderFailed(error) => {
            state
                .messages
                .set_error(format!("Failed to open folder: {error}"));

            DispatchResult::handled()
        }

        Action::ResultNextPage => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
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
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
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
    use crate::services::AppServices;
    use crate::update::test_fixtures;
    use std::sync::Arc;

    use crate::model::browse::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
    use crate::update::browse::query::dispatch_query;
    use crate::update::browse::query::tests::*;
    use crate::update::reducer::reduce;

    fn csv_succeeded_action(state: &mut AppState, path: &str, row_count: Option<usize>) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExportSucceeded {
            run_id,
            path: path.to_string(),
            row_count,
        }
    }

    fn csv_failed_action(state: &mut AppState, error: DbOperationError) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExportFailed { run_id, error }
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
            state.query.pagination.reset_for_table("public", "users");
            let now = Instant::now();

            let effects = dispatch_query(&mut state, &Action::ResultNextPage, now).unwrap();

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

            let effects = dispatch_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_for_adhoc() {
            let mut state = create_test_state();
            state.query.set_current_result(adhoc_result());
            let now = Instant::now();

            let effects = dispatch_query(&mut state, &Action::ResultNextPage, now).unwrap();

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

            let effects = dispatch_query(&mut state, &Action::ResultNextPage, now).unwrap();

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

            dispatch_query(&mut state, &Action::ResultNextPage, Instant::now());

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
            state.query.pagination.reset_for_table("public", "users");
            state.result_interaction.activate_cell(3, 1);
            state.result_interaction.stage_row(3);

            dispatch_query(&mut state, &Action::ResultNextPage, Instant::now());

            assert!(state.result_interaction.selection().row().is_none());
            assert!(state.result_interaction.selection().cell().is_none());
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn prev_then_next_reopens_the_page_after_an_empty_forward_result() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state.query.pagination.reset_for_table("public", "users");

            let next_effects =
                dispatch_query(&mut state, &Action::ResultNextPage, Instant::now()).unwrap();
            assert!(matches!(
                next_effects.first(),
                Some(Effect::ExecutePreview { target_page: 1, .. })
            ));

            let next_result =
                query_completed_action(&mut state, preview_result(PREVIEW_PAGE_SIZE), 0, Some(1));
            dispatch_query(&mut state, &next_result, Instant::now());

            let empty_next_result =
                query_completed_action(&mut state, preview_result(0), 0, Some(2));
            dispatch_query(&mut state, &empty_next_result, Instant::now());
            assert_eq!(state.query.pagination.current_page(), 1);
            assert!(state.query.pagination.reached_end());

            let prev_effects =
                dispatch_query(&mut state, &Action::ResultPrevPage, Instant::now()).unwrap();
            assert!(matches!(
                prev_effects.first(),
                Some(Effect::ExecutePreview { target_page: 0, .. })
            ));
            assert!(!state.query.pagination.reached_end());

            let prev_result =
                query_completed_action(&mut state, preview_result(PREVIEW_PAGE_SIZE), 0, Some(0));
            dispatch_query(&mut state, &prev_result, Instant::now());

            let next_effects =
                dispatch_query(&mut state, &Action::ResultNextPage, Instant::now()).unwrap();
            assert!(matches!(
                next_effects.first(),
                Some(Effect::ExecutePreview { target_page: 1, .. })
            ));
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
            state.query.pagination.reset_for_table("public", "users");
            state.query.pagination.set_page_result(2, false);
            let now = Instant::now();

            let effects = dispatch_query(&mut state, &Action::ResultPrevPage, now).unwrap();

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

            let effects = dispatch_query(&mut state, &Action::ResultPrevPage, now).unwrap();

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

            dispatch_query(&mut state, &Action::ResultPrevPage, Instant::now());

            assert_eq!(state.result_interaction.selection().row(), Some(1));
            assert_eq!(state.result_interaction.selection().cell(), Some(1));
            assert!(state.result_interaction.staged_delete_rows().contains(&1));
        }
    }

    mod csv_export {
        use super::*;
        use crate::domain::QueryResult;
        use rstest::rstest;

        #[test]
        fn delete_success_then_csv_export_confirmation_then_preview_completion_clears_selection() {
            let mut state = test_fixtures::state_after_delete_success();
            state.query.set_current_result(preview_result(10));
            state.query.pagination.reset_for_table("public", "users");

            let now = Instant::now();
            let effects = reduce(
                &mut state,
                Action::RequestCsvExport,
                now,
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
            assert_eq!(
                state.query.post_delete_row_selection(),
                PostDeleteRowSelection::Keep
            );
            test_fixtures::complete_table_preview(&mut state, now);
            assert!(state.result_interaction.selection().row().is_none());
            assert!(state.result_interaction.selection().cell().is_none());
        }

        #[test]
        fn request_with_adhoc_result_uses_original_query() {
            let mut state = create_test_state();
            state.query.set_current_result(adhoc_result());

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
            let Some(ConfirmIntent::CsvExportRerunnable {
                export_query,
                file_name,
                ..
            }) = state.confirm_dialog.intent()
            else {
                panic!("expected rerunnable CSV export confirmation");
            };
            assert_eq!(export_query, "SELECT 1");
            assert_eq!(file_name, "adhoc");
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

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
        }

        #[test]
        fn request_without_result_is_noop() {
            let mut state = create_test_state();
            state.query.clear_current_result();

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn rerunnable_export_always_confirms_even_when_result_is_small() {
            let mut state = create_test_state();
            state.query.set_current_result(adhoc_result());

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
        }

        #[test]
        fn rerunnable_export_always_confirms_even_when_result_is_large() {
            let mut state = create_test_state();
            let result = QueryResult::success(
                "SELECT 1".to_string(),
                vec!["value".to_string()],
                vec![vec!["1".to_string()]],
                0,
                QuerySource::Adhoc,
            )
            .with_row_count(200_000);
            state.query.set_current_result(Arc::new(result));

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
        }

        #[test]
        fn export_succeeded_sets_success_message() {
            let mut state = create_test_state();
            let action = csv_succeeded_action(&mut state, "/tmp/export.csv", Some(42));

            let effects = dispatch_query(&mut state, &action, Instant::now()).unwrap();

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
        fn stale_export_success_after_connection_switch_is_ignored() {
            let mut state = create_test_state();
            let stale_run_id = begin_query_run(&mut state);

            state.session.reset(&mut state.query);
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/other");
            let current_run_id = begin_query_run(&mut state);

            let effects = dispatch_query(
                &mut state,
                &Action::CsvExportSucceeded {
                    run_id: stale_run_id,
                    path: "/tmp/stale-export.csv".to_string(),
                    row_count: Some(1),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(stale_run_id < current_run_id);
            assert!(effects.is_empty());
            assert!(state.query.is_running());
            assert!(state.messages.last_success.is_none());
        }

        #[test]
        fn export_failed_sets_error_message() {
            let mut state = create_test_state();
            let action = csv_failed_action(
                &mut state,
                DbOperationError::QueryFailed("psql error".to_string()),
            );

            let effects = dispatch_query(&mut state, &action, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Query failed: psql error. Review the database error details and SQL.")
            );
        }

        #[test]
        fn stale_export_failure_after_connection_switch_is_ignored() {
            let mut state = create_test_state();
            let stale_run_id = begin_query_run(&mut state);

            state.session.reset(&mut state.query);
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/other");
            let current_run_id = begin_query_run(&mut state);

            let effects = dispatch_query(
                &mut state,
                &Action::CsvExportFailed {
                    run_id: stale_run_id,
                    error: DbOperationError::QueryFailed("stale export".to_string()),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(stale_run_id < current_run_id);
            assert!(effects.is_empty());
            assert!(state.query.is_running());
            assert!(state.messages.last_error.is_none());
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

            let effects =
                dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

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

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

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

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

                assert_eq!(effects.len(), 1);
                assert!(matches!(&effects[0], Effect::ExportCsvFromCache { .. }));
            }

            #[test]
            fn select_always_asks_for_unknown_row_count() {
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

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

                assert!(effects.is_empty());
                assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
                assert!(state.confirm_dialog.message().contains("unknown"));
            }

            #[test]
            fn preview_exports_visible_typed_values_from_cache() {
                let mut state = sqlite_state();
                state.query.set_current_result(Arc::new(
                    QueryResult::success_with_values(
                        "SELECT CASE WHEN typeof(\"message\") = 'text' THEN hex(\"message\") END AS \"message\" FROM \"logs\"".to_string(),
                        vec!["message".to_string()],
                        vec![vec![QueryValue::text("a\0bc")]],
                        1,
                        QuerySource::Preview,
                    ),
                ));

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

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

        mod mysql {
            use super::*;

            fn mysql_state(query: &str) -> AppState {
                let mut state = AppState::new("test_project".to_string());
                test_fixtures::activate_mysql_connection(&mut state, "mysql://localhost/test");
                state
                    .query
                    .set_current_result(Arc::new(QueryResult::success(
                        query.to_string(),
                        vec!["column".to_string()],
                        vec![vec!["value".to_string()]],
                        1,
                        QuerySource::Adhoc,
                    )));
                state
            }

            #[rstest]
            #[case::select("SELECT 1")]
            #[case::table("TABLE users")]
            #[case::show("SHOW TABLES")]
            #[case::describe("DESCRIBE users")]
            fn supported_rerunnable_queries_ask_for_unknown_row_count(#[case] query: &str) {
                let mut state = mysql_state(query);

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

                assert!(effects.is_empty());
                assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
                assert!(state.confirm_dialog.message().contains("unknown"));
            }

            #[test]
            fn preview_exports_visible_typed_values_from_cache() {
                let mut state = AppState::new("test_project".to_string());
                test_fixtures::activate_mysql_connection(&mut state, "mysql://localhost/test");
                let values = vec![vec![
                    QueryValue::Blob(vec![0x00, 0xFF, 0xA1]),
                    QueryValue::text("0x00FFA1"),
                    QueryValue::Null,
                    QueryValue::text("text"),
                ]];
                state
                    .query
                    .set_current_result(Arc::new(QueryResult::success_with_values(
                        "SELECT payload, text_value, nullable, text FROM users".to_string(),
                        vec![
                            "payload".to_string(),
                            "text_value".to_string(),
                            "nullable".to_string(),
                            "text".to_string(),
                        ],
                        values.clone(),
                        1,
                        QuerySource::Preview,
                    )));

                let effects =
                    dispatch_query(&mut state, &Action::RequestCsvExport, Instant::now()).unwrap();

                let Effect::ExportCsvFromCache {
                    columns,
                    values: cached_values,
                    row_count,
                    ..
                } = &effects[0]
                else {
                    panic!("expected cached CSV export effect");
                };
                assert_eq!(columns, &["payload", "text_value", "nullable", "text"]);
                assert_eq!(cached_values, &values);
                assert_eq!(*row_count, Some(1));
            }
        }
    }
}
