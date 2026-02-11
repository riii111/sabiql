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

        _ => None,
    }
}
