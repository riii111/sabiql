use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::query_execution::PREVIEW_PAGE_SIZE;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::services::AppServices;
use crate::app::update::action::Action;
use crate::domain::QuerySource;

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    _services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::RequestCsvExport => {
            if !state.can_request_csv_export() {
                return Some(vec![]);
            }
            let Some(result) = state.query.visible_result() else {
                return Some(vec![]);
            };
            let dsn = match &state.session.dsn {
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
            let count_query = format!("SELECT COUNT(*) FROM ({stripped}) AS _export_count");

            Some(vec![Effect::CountRowsForExport {
                dsn,
                count_query,
                export_query,
                file_name,
                read_only: state.session.read_only,
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
                    Some(n) => format!("Export {n} rows to CSV? This may take a while."),
                    None => "Row count unknown. Export to CSV?".to_string(),
                };
                state.confirm_dialog.open(
                    "Confirm CSV Export",
                    msg,
                    crate::app::model::shared::confirm_dialog::ConfirmIntent::CsvExport {
                        export_query: export_query.clone(),
                        file_name: file_name.clone(),
                        row_count: *row_count,
                    },
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
                Some(vec![])
            } else {
                let dsn = match &state.session.dsn {
                    Some(d) => d.clone(),
                    None => return Some(vec![]),
                };
                Some(vec![Effect::ExportCsv {
                    dsn,
                    query: export_query.clone(),
                    file_name: file_name.clone(),
                    row_count: *row_count,
                    read_only: state.session.read_only,
                }])
            }
        }

        Action::ExecuteCsvExport {
            export_query,
            file_name,
            row_count,
        } => {
            let dsn = match &state.session.dsn {
                Some(d) => d.clone(),
                None => return Some(vec![]),
            };
            Some(vec![Effect::ExportCsv {
                dsn,
                query: export_query.clone(),
                file_name: file_name.clone(),
                row_count: *row_count,
                read_only: state.session.read_only,
            }])
        }

        Action::CsvExportSucceeded { path, row_count } => {
            let msg = match row_count {
                Some(n) => format!("Exported {n} rows → {path}"),
                None => format!("Exported → {path}"),
            };
            state.messages.set_success_at(msg, now);
            let folder = Path::new(path)
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            Some(vec![Effect::OpenFolder { path: folder }])
        }

        Action::CsvExportFailed(error) => {
            state.messages.set_error_at(error.to_string(), now);
            Some(vec![])
        }

        Action::OpenFolderFailed(error) => {
            state
                .messages
                .set_error_at(format!("Failed to open folder: {error}"), now);

            Some(vec![])
        }

        Action::ResultNextPage => {
            if state.query.is_running() || !state.query.can_paginate_visible_result() {
                return Some(vec![]);
            }
            if !state.query.pagination.can_next() {
                return Some(vec![]);
            }
            if let Some(dsn) = state.session.dsn.clone() {
                let next_page = state.query.pagination.current_page + 1;
                state.query.begin_running(now);
                state.result_interaction.reset_view();
                Some(vec![Effect::ExecutePreview {
                    dsn,
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.session.selection_generation(),
                    limit: PREVIEW_PAGE_SIZE,
                    offset: next_page * PREVIEW_PAGE_SIZE,
                    target_page: next_page,
                    read_only: state.session.read_only,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ResultPrevPage => {
            if state.query.is_running() || !state.query.can_paginate_visible_result() {
                return Some(vec![]);
            }
            if !state.query.pagination.can_prev() {
                return Some(vec![]);
            }
            if let Some(dsn) = state.session.dsn.clone() {
                let prev_page = state.query.pagination.current_page - 1;
                state.query.begin_running(now);
                state.result_interaction.reset_view();
                state.query.pagination.reached_end = false;
                Some(vec![Effect::ExecutePreview {
                    dsn,
                    schema: state.query.pagination.schema.clone(),
                    table: state.query.pagination.table.clone(),
                    generation: state.session.selection_generation(),
                    limit: PREVIEW_PAGE_SIZE,
                    offset: prev_page * PREVIEW_PAGE_SIZE,
                    target_page: prev_page,
                    read_only: state.session.read_only,
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
    use crate::domain::{QueryResult, QuerySource};
    use std::sync::Arc;

    use crate::app::model::browse::query_execution::PaginationState;
    use crate::app::update::browse::query::reduce_query;
    use crate::app::update::browse::query::tests::*;

    fn preview_result_with_two_columns(row_count: usize) -> Arc<QueryResult> {
        let rows: Vec<Vec<String>> = (0..row_count)
            .map(|i| vec![i.to_string(), format!("name_{i}")])
            .collect();
        Arc::new(QueryResult {
            query: "SELECT * FROM users".to_string(),
            columns: vec!["id".to_string(), "name".to_string()],
            rows,
            row_count,
            execution_time_ms: 10,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        })
    }

    mod next_page {
        use super::*;

        #[test]
        fn emits_correct_offset_for_next_page() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
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
                other => panic!("expected ExecutePreview, got {other:?}"),
            }
        }

        #[test]
        fn noop_when_reached_end() {
            let mut state = create_test_state();
            state.query.set_current_result(preview_result(100));
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
            state.query.set_current_result(adhoc_result());
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
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
            state.query.begin_running(Instant::now());
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
        fn preserves_view_state_when_next_page_noops() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(100));
            state.query.pagination.reached_end = true;
            state.result_interaction.activate_cell(2, 1);
            state.result_interaction.stage_row(2);

            reduce_query(
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
            state.query.pagination = PaginationState {
                current_page: 0,
                total_rows_estimate: Some(1500),
                reached_end: false,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            state.result_interaction.activate_cell(3, 1);
            state.result_interaction.stage_row(3);

            reduce_query(
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
                other => panic!("expected ExecutePreview, got {other:?}"),
            }
        }

        #[test]
        fn noop_on_first_page() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result(PREVIEW_PAGE_SIZE));
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

        #[test]
        fn preserves_view_state_when_prev_page_noops() {
            let mut state = create_test_state();
            state
                .query
                .set_current_result(preview_result_with_two_columns(PREVIEW_PAGE_SIZE));
            state.query.pagination.current_page = 0;
            state.result_interaction.activate_cell(1, 1);
            state.result_interaction.stage_row(1);

            reduce_query(
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
        use crate::app::ports::DbOperationError;
        use crate::domain::QueryResult;

        fn export_test_state() -> AppState {
            let mut state = AppState::new("test_project".to_string());
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state
        }

        #[test]
        fn request_with_preview_result_emits_count_effect() {
            let mut state = export_test_state();
            state.query.set_current_result(preview_result(10));
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
                other => panic!("expected CountRowsForExport, got {other:?}"),
            }
        }

        #[test]
        fn request_without_result_is_noop() {
            let mut state = create_test_state();
            state.query.clear_current_result();

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
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.title().contains("CSV Export"));
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
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.confirm_dialog.message().contains("unknown"));
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
                &Action::CsvExportFailed(DbOperationError::QueryFailed("psql error".to_string())),
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Query failed: psql error")
            );
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
}
