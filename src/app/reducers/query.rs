//! Query sub-reducer: query execution and command line.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::action::Action;
use crate::app::command::{command_to_action, parse_command};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::{PREVIEW_PAGE_SIZE, QueryStatus};
use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::domain::{QueryResult, QuerySource};

/// Handles query execution and command line actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_query(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
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
                state.query.result_highlight_until = Some(now + Duration::from_millis(500));
                state.query.history_index = None;

                if result.source == QuerySource::Adhoc {
                    if result.is_error() {
                        state.sql_modal.status = SqlModalStatus::Error;
                    } else {
                        state.sql_modal.status = SqlModalStatus::Success;
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
            }
            Some(vec![])
        }
        Action::QueryFailed(error, generation) => {
            if *generation == 0 || *generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_selection.reset();
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
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
            }
            Some(vec![])
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.ui.input_mode = InputMode::Normal;
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
                _ => vec![],
            })
        }

        Action::ExecutePreview {
            schema,
            table,
            generation,
        } => {
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
        })
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

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

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

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_for_adhoc() {
            let mut state = create_test_state();
            state.query.current_result = Some(adhoc_result());
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn noop_when_running() {
            let mut state = create_test_state();
            state.query.current_result = Some(preview_result(PREVIEW_PAGE_SIZE));
            state.query.status = QueryStatus::Running;
            let now = Instant::now();

            let effects = reduce_query(&mut state, &Action::ResultNextPage, now).unwrap();

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

            let effects = reduce_query(&mut state, &Action::ResultPrevPage, now).unwrap();

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

            let effects = reduce_query(&mut state, &Action::ResultPrevPage, now).unwrap();

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

            let _ = reduce_query(
                &mut state,
                &Action::ExecutePreview {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                },
                now,
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

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(2),
                },
                now,
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

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 1,
                    target_page: Some(0),
                },
                now,
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

            let _ = reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                now,
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

            let _ = reduce_query(
                &mut state,
                &Action::QueryFailed("error".to_string(), 1),
                Instant::now(),
            );

            assert_eq!(state.ui.result_selection.mode(), ResultNavMode::Scroll);
            assert_eq!(state.ui.result_scroll_offset, 0);
            assert_eq!(state.ui.result_horizontal_offset, 0);
        }
    }
}
