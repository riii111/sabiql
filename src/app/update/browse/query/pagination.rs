use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::QuerySource;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::DbOperationError;
use crate::update::action::CsvExport;
use crate::update::browse::query::preview_effect_for_current_table;
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

pub(in crate::update) fn request_csv_export(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if !state.can_request_csv_export() {
        return vec![];
    }
    let Some(result) = state.query.current_result().cloned() else {
        return vec![];
    };
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };

    let file_name = csv_export_file_name(state, result.source);
    let row_count = result.data_row_count();
    let run_id = state.query.begin_non_preview_running(now);

    if row_count > LARGE_EXPORT_THRESHOLD {
        state.confirm_dialog.open(
            "Confirm CSV Export",
            format!("Export {row_count} rows to CSV? This may take a while."),
            ConfirmIntent::CsvExportCached {
                endpoint,
                run_id,
                file_name,
                result,
            },
        );
        state.modal.push_mode(InputMode::ConfirmDialog);
        vec![]
    } else {
        vec![Effect::ExportCsvFromCache {
            run_id,
            file_name,
            result,
        }]
    }
}

pub(in crate::update) fn csv_exported(
    state: &mut AppState,
    run_id: u64,
    outcome: Result<CsvExport, DbOperationError>,
    now: Instant,
) -> Vec<Effect> {
    if !state.query.is_current_run(run_id) {
        return vec![];
    }

    state.query.mark_idle();
    let CsvExport { path, row_count } = match outcome {
        Ok(export) => export,
        Err(error) => {
            state.messages.set_error(error.user_message());
            return vec![];
        }
    };
    state
        .messages
        .set_success_at(format!("Exported {row_count} rows → {path}"), now);
    let folder = Path::new(&path)
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    vec![Effect::OpenFolder { path: folder }]
}

pub(in crate::update) fn result_next_page(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if state.query.is_running() || !state.query.can_paginate_visible_result() {
        return vec![];
    }
    if !state.query.pagination.can_next() {
        return vec![];
    }
    let next_page = state.query.pagination.next_page();
    let generation = state.session.selection_generation();
    match preview_effect_for_current_table(state, now, next_page, generation) {
        Some(effect) => {
            state.result_interaction.reset_view();
            vec![effect]
        }
        None => vec![],
    }
}

pub(in crate::update) fn result_prev_page(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if state.query.is_running() || !state.query.can_paginate_visible_result() {
        return vec![];
    }
    if !state.query.pagination.can_prev() {
        return vec![];
    }
    let prev_page = state.query.pagination.prev_page();
    let generation = state.session.selection_generation();
    match preview_effect_for_current_table(state, now, prev_page, generation) {
        Some(effect) => {
            state.result_interaction.reset_view();
            state.query.pagination.clear_reached_end();
            vec![effect]
        }
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{QueryResult, QuerySource};
    use crate::ports::outbound::DbOperationError;
    use crate::services::AppServices;
    use crate::update::action::Action;
    use crate::update::test_fixtures;
    use std::sync::Arc;

    use crate::model::browse::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
    use crate::update::browse::query::tests::*;
    use crate::update::reducer::{reduce, route};

    fn csv_succeeded_action(state: &mut AppState, path: &str, row_count: usize) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExported {
            run_id,
            outcome: Ok(CsvExport {
                path: path.to_string(),
                row_count,
            }),
        }
    }

    fn csv_failed_action(state: &mut AppState, error: DbOperationError) -> Action {
        let run_id = begin_query_run(state);
        Action::CsvExported {
            run_id,
            outcome: Err(error),
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
            state.query.pagination.reset_for_table("public", "users");
            let now = Instant::now();

            let effects = route(
                &mut state,
                Action::ResultNextPage,
                now,
                &AppServices::stub(),
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
        fn noop_for_adhoc() {
            let mut state = create_test_state();
            state.query.set_current_result(adhoc_result());
            let now = Instant::now();

            let effects = route(
                &mut state,
                Action::ResultNextPage,
                now,
                &AppServices::stub(),
            );

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

            let effects = route(
                &mut state,
                Action::ResultNextPage,
                now,
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
        }

        #[test]
        fn next_on_last_page_preserves_selection_and_staged_rows() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(100));
            state.query.pagination.set_page_result(0, true);
            state.result_interaction.activate_cell(2, 1);
            state.result_interaction.stage_row(2);

            let effects = route(
                &mut state,
                Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
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

            route(
                &mut state,
                Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );

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

            let next_effects = route(
                &mut state,
                Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(matches!(
                next_effects.first(),
                Some(Effect::ExecutePreview { target_page: 1, .. })
            ));

            let next_result =
                query_completed_action(&mut state, preview_result(PREVIEW_PAGE_SIZE), 0, Some(1));
            route(
                &mut state,
                next_result,
                Instant::now(),
                &AppServices::stub(),
            );

            let empty_next_result =
                query_completed_action(&mut state, preview_result(0), 0, Some(2));
            route(
                &mut state,
                empty_next_result,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.query.pagination.current_page(), 1);
            assert!(state.query.pagination.reached_end());

            let prev_effects = route(
                &mut state,
                Action::ResultPrevPage,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(matches!(
                prev_effects.first(),
                Some(Effect::ExecutePreview { target_page: 0, .. })
            ));
            assert!(!state.query.pagination.reached_end());

            let prev_result =
                query_completed_action(&mut state, preview_result(PREVIEW_PAGE_SIZE), 0, Some(0));
            route(
                &mut state,
                prev_result,
                Instant::now(),
                &AppServices::stub(),
            );

            let next_effects = route(
                &mut state,
                Action::ResultNextPage,
                Instant::now(),
                &AppServices::stub(),
            );
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

            let effects = route(
                &mut state,
                Action::ResultPrevPage,
                now,
                &AppServices::stub(),
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
        fn prev_on_first_page_preserves_selection_and_staged_rows() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(PREVIEW_PAGE_SIZE));
            state.query.pagination.set_page_result(0, false);
            state.result_interaction.activate_cell(1, 1);
            state.result_interaction.stage_row(1);

            let effects = route(
                &mut state,
                Action::ResultPrevPage,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert_eq!(state.result_interaction.selection().row(), Some(1));
            assert_eq!(state.result_interaction.selection().cell(), Some(1));
            assert!(state.result_interaction.staged_delete_rows().contains(&1));
        }
    }

    mod csv_export {
        use super::*;
        use crate::domain::QueryValue;
        use rstest::rstest;

        fn activate_postgres(state: &mut AppState) {
            test_fixtures::activate_postgres_connection(state, "postgres://localhost/test");
        }

        fn activate_sqlite(state: &mut AppState) {
            test_fixtures::activate_sqlite_connection(state, "sqlite:///tmp/test.db");
        }

        fn activate_mysql(state: &mut AppState) {
            test_fixtures::activate_mysql_connection(state, "mysql://localhost/test");
        }

        fn request_csv_export(state: &mut AppState) -> Vec<Effect> {
            route(
                state,
                Action::RequestCsvExport,
                Instant::now(),
                &AppServices::stub(),
            )
        }

        fn typed_result(source: QuerySource, values: Vec<Vec<QueryValue>>) -> Arc<QueryResult> {
            Arc::new(QueryResult::success_with_values(
                "SELECT payload, text_value, nullable FROM users".to_string(),
                vec![
                    "payload".to_string(),
                    "text_value".to_string(),
                    "nullable".to_string(),
                ],
                values,
                1,
                source,
            ))
        }

        fn rows_of_null(row_count: usize) -> Arc<QueryResult> {
            Arc::new(QueryResult::success_with_values(
                "SELECT id FROM users".to_string(),
                vec!["id".to_string()],
                vec![vec![QueryValue::Null]; row_count],
                1,
                QuerySource::Adhoc,
            ))
        }

        #[test]
        fn delete_success_then_csv_export_then_preview_completion_clears_selection() {
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

            assert!(matches!(
                effects.as_slice(),
                [Effect::ExportCsvFromCache { .. }]
            ));
            assert_eq!(
                state.query.post_delete_row_selection(),
                PostDeleteRowSelection::Keep
            );
            test_fixtures::complete_table_preview(&mut state, now);
            assert!(state.result_interaction.selection().row().is_none());
            assert!(state.result_interaction.selection().cell().is_none());
        }

        #[rstest]
        #[case::postgres(activate_postgres as fn(&mut AppState))]
        #[case::sqlite(activate_sqlite as fn(&mut AppState))]
        #[case::mysql(activate_mysql as fn(&mut AppState))]
        fn adhoc_result_is_exported_as_displayed_without_confirmation(
            #[case] activate: fn(&mut AppState),
        ) {
            let mut state = AppState::new("test_project".to_string());
            activate(&mut state);
            let values = vec![
                vec![
                    QueryValue::Blob(vec![0x00, 0xFF, 0xA1]),
                    QueryValue::text("0x00FFA1"),
                    QueryValue::Null,
                ],
                vec![
                    QueryValue::text("a\0bc"),
                    QueryValue::text(" trailing "),
                    QueryValue::text("2"),
                ],
            ];
            let displayed = typed_result(QuerySource::Adhoc, values.clone());
            state.query.set_current_result(Arc::clone(&displayed));

            let effects = request_csv_export(&mut state);

            let [
                Effect::ExportCsvFromCache {
                    file_name, result, ..
                },
            ] = effects.as_slice()
            else {
                panic!("expected one CSV export effect, got {effects:?}");
            };
            assert_eq!(file_name, "adhoc");
            assert!(Arc::ptr_eq(result, &displayed));
            assert_eq!(result.values(), values.as_slice());
            assert_ne!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.query.is_running());
        }

        #[test]
        fn preview_page_is_exported_with_table_derived_file_name() {
            let mut state = create_test_state();
            state
                .query
                .pagination
                .reset_for_table("public", "user logs");
            state.query.set_current_result(preview_result(3));

            let effects = request_csv_export(&mut state);

            let [
                Effect::ExportCsvFromCache {
                    file_name, result, ..
                },
            ] = effects.as_slice()
            else {
                panic!("expected one CSV export effect, got {effects:?}");
            };
            assert_eq!(file_name, "user_logs");
            assert_eq!(result.data_row_count(), 3);
        }

        #[rstest]
        #[case::insert("INSERT INTO users(name) VALUES ('a') RETURNING id")]
        #[case::update("UPDATE users SET name = 'b' WHERE id = 1 RETURNING id")]
        #[case::delete("DELETE FROM users WHERE id = 1 RETURNING id")]
        fn tabular_result_of_executed_dml_is_exported(#[case] query: &str) {
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

            let effects = request_csv_export(&mut state);

            assert!(matches!(
                effects.as_slice(),
                [Effect::ExportCsvFromCache { .. }]
            ));
        }

        #[rstest]
        #[case::postgres(activate_postgres as fn(&mut AppState))]
        #[case::sqlite(activate_sqlite as fn(&mut AppState))]
        #[case::mysql(activate_mysql as fn(&mut AppState))]
        fn zero_row_result_exports_header_only(#[case] activate: fn(&mut AppState)) {
            let mut state = AppState::new("test_project".to_string());
            activate(&mut state);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT id, name FROM users WHERE false".to_string(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![],
                    1,
                    QuerySource::Adhoc,
                )));

            let effects = request_csv_export(&mut state);

            let [Effect::ExportCsvFromCache { result, .. }] = effects.as_slice() else {
                panic!("expected one CSV export effect, got {effects:?}");
            };
            assert_eq!(result.columns, ["id", "name"]);
            assert!(result.values().is_empty());
        }

        #[rstest]
        #[case::postgres(activate_postgres as fn(&mut AppState))]
        #[case::sqlite(activate_sqlite as fn(&mut AppState))]
        #[case::mysql(activate_mysql as fn(&mut AppState))]
        fn result_without_columns_is_not_exported(#[case] activate: fn(&mut AppState)) {
            let mut state = AppState::new("test_project".to_string());
            activate(&mut state);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "UPDATE users SET name = 'b'".to_string(),
                    vec![],
                    vec![],
                    1,
                    QuerySource::Adhoc,
                )));

            let effects = request_csv_export(&mut state);

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
            assert_ne!(state.input_mode(), InputMode::ConfirmDialog);
        }

        #[test]
        fn request_without_result_is_noop() {
            let mut state = create_test_state();
            state.query.clear_current_result();

            let effects = request_csv_export(&mut state);

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
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

            let effects = request_csv_export(&mut state);

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
        }

        #[test]
        fn row_count_at_threshold_exports_without_confirmation() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(rows_of_null(LARGE_EXPORT_THRESHOLD));

            let effects = request_csv_export(&mut state);

            assert!(matches!(
                effects.as_slice(),
                [Effect::ExportCsvFromCache { .. }]
            ));
            assert_ne!(state.input_mode(), InputMode::ConfirmDialog);
        }

        #[test]
        fn row_count_above_threshold_confirms_with_exact_count_and_keeps_displayed_result() {
            let mut state = create_test_state();
            let displayed = rows_of_null(LARGE_EXPORT_THRESHOLD + 1);
            state.query.set_current_result(Arc::clone(&displayed));

            let effects = request_csv_export(&mut state);

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("100001"));
            let Some(ConfirmIntent::CsvExportCached { result, .. }) = state.confirm_dialog.intent()
            else {
                panic!("expected CSV export confirmation");
            };
            assert!(Arc::ptr_eq(result, &displayed));
        }

        #[test]
        fn export_succeeded_sets_success_message() {
            let mut state = create_test_state();
            let action = csv_succeeded_action(&mut state, "/tmp/export.csv", 42);

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::OpenFolder { .. }));
            assert_eq!(
                state.messages.last_success.as_deref(),
                Some("Exported 42 rows → /tmp/export.csv")
            );
        }

        #[test]
        fn stale_export_success_after_connection_switch_is_ignored() {
            let mut state = create_test_state();
            let stale_run_id = begin_query_run(&mut state);

            state.session.reset(&mut state.query);
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/other");
            let current_run_id = begin_query_run(&mut state);

            let effects = route(
                &mut state,
                Action::CsvExported {
                    run_id: stale_run_id,
                    outcome: Ok(CsvExport {
                        path: "/tmp/stale-export.csv".to_string(),
                        row_count: 1,
                    }),
                },
                Instant::now(),
                &AppServices::stub(),
            );

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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

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

            let effects = route(
                &mut state,
                Action::CsvExported {
                    run_id: stale_run_id,
                    outcome: Err(DbOperationError::QueryFailed("stale export".to_string())),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(stale_run_id < current_run_id);
            assert!(effects.is_empty());
            assert!(state.query.is_running());
            assert!(state.messages.last_error.is_none());
        }
    }
}
