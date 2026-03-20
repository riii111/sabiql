use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::action::{Action, TableTarget};
use crate::app::command::{command_to_action, parse_command};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
use crate::app::services::AppServices;
use crate::app::sql_modal_context::{AdhocSuccessSnapshot, SqlModalStatus};
use crate::app::state::AppState;
use crate::domain::{QueryResult, QuerySource};

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
    let Some(dsn) = state.session.dsn.clone() else {
        return vec![];
    };

    let mut effects = vec![];

    if tag.is_schema_modifying() {
        state.sql_modal.reset_prefetch();
        state.session.set_table_detail_raw(None);

        effects.push(Effect::CacheInvalidate { dsn: dsn.clone() });
        effects.push(Effect::ClearCompletionEngineCache);
        effects.push(Effect::FetchMetadata { dsn });
    } else if !state.query.pagination.table.is_empty() {
        let page = state.query.pagination.current_page;
        effects.push(Effect::ExecutePreview {
            dsn,
            schema: state.query.pagination.schema.clone(),
            table: state.query.pagination.table.clone(),
            generation: state.session.selection_generation(),
            limit: PREVIEW_PAGE_SIZE,
            offset: page * PREVIEW_PAGE_SIZE,
            target_page: page,
            read_only: state.session.read_only,
        });
    }

    effects
}

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    _services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::QueryCompleted {
            result,
            generation,
            target_page,
        } => {
            if *generation == 0 || *generation == state.session.selection_generation() {
                state.query.mark_idle();

                let is_adhoc_error = result.source == QuerySource::Adhoc && result.is_error();
                if !is_adhoc_error {
                    state.result_interaction.reset_view();
                    state
                        .query
                        .set_result_highlight(now + Duration::from_millis(500));
                    state.query.exit_history();
                }

                if result.source == QuerySource::Adhoc {
                    if result.is_error() {
                        state
                            .sql_modal
                            .mark_adhoc_error(result.error.clone().unwrap_or_default());
                    } else {
                        state.sql_modal.mark_adhoc_success(AdhocSuccessSnapshot {
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

                if !result.is_error() || result.source != QuerySource::Adhoc {
                    state.query.set_current_result(Arc::clone(result));
                }

                if result.source == QuerySource::Preview {
                    match state.query.post_delete_row_selection() {
                        PostDeleteRowSelection::Keep => {}
                        PostDeleteRowSelection::Clear => {
                            state.result_interaction.exit_row_to_scroll();
                        }
                        PostDeleteRowSelection::Select(row) => {
                            if !result.rows.is_empty() {
                                let clamped = row.min(result.rows.len() - 1);
                                state.result_interaction.enter_row(clamped);

                                let visible = state.result_visible_rows();
                                if visible > 0 && clamped >= visible {
                                    state.result_interaction.scroll_offset = clamped - visible + 1;
                                }
                            }
                        }
                    }
                    state
                        .query
                        .set_post_delete_selection(PostDeleteRowSelection::Keep);
                }

                Some(try_adhoc_refresh(state, result))
            } else {
                Some(vec![])
            }
        }
        Action::QueryFailed(error, generation) => {
            if *generation == 0 || *generation == state.session.selection_generation() {
                state.query.mark_idle();
                let is_adhoc = state.modal.active_mode() == InputMode::SqlModal;
                if !is_adhoc {
                    state.result_interaction.reset_view();
                    state
                        .query
                        .set_post_delete_selection(PostDeleteRowSelection::Keep);
                    state.query.clear_delete_refresh_target();
                }
                state.set_error(error.clone());
                if is_adhoc {
                    state.sql_modal.mark_adhoc_error(error.clone());
                }
            }
            Some(vec![])
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.modal.pop_mode();
            state.command_line_input.clear();

            Some(match follow_up {
                Action::Quit => {
                    state.should_quit = true;
                    vec![]
                }
                Action::OpenHelp => {
                    state.modal.set_mode(InputMode::Help);
                    vec![]
                }
                Action::OpenSqlModal => {
                    state.modal.set_mode(InputMode::SqlModal);
                    state.sql_modal.set_status(SqlModalStatus::Normal);
                    if !state.sql_modal.is_prefetch_started() && state.session.metadata().is_some()
                    {
                        vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
                    } else {
                        vec![]
                    }
                }
                Action::OpenErTablePicker => {
                    vec![Effect::DispatchActions(vec![Action::OpenErTablePicker])]
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
            if let Some(dsn) = &state.session.dsn {
                state.query.begin_running(now);

                state.query.pagination.reset();
                state.query.pagination.schema = schema.clone();
                state.query.pagination.table = table.clone();

                let row_estimate = state
                    .session
                    .table_detail()
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
                    read_only: state.session.read_only,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.session.dsn {
                state.query.begin_running(now);
                Some(vec![Effect::ExecuteAdhoc {
                    dsn: dsn.clone(),
                    query: query.clone(),
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
    use crate::app::query_execution::PaginationState;
    use crate::app::reducers::query::reduce_query;
    use crate::app::reducers::query::tests::*;

    mod command_line_submit {
        use super::*;

        #[test]
        fn submit_quit_pops_mode_and_sets_quit() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input = "q".to_string();

            reduce_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.should_quit);
        }

        #[test]
        fn submit_unknown_pops_mode_without_side_effects() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::CellEdit);
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input = "unknown_cmd".to_string();

            reduce_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert!(!state.should_quit);
        }

        #[test]
        fn submit_erd_dispatches_open_er_table_picker() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input = "erd".to_string();

            let effects = reduce_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.command_line_input.is_empty());
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::DispatchActions(actions) => {
                    assert!(matches!(actions[0], Action::OpenErTablePicker));
                }
                other => panic!("expected DispatchActions, got {:?}", other),
            }
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
            state.session.set_selection_generation(1);
            let result = preview_result(100);
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
            state.session.set_selection_generation(1);
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

            assert_eq!(state.query.pagination.current_page, 3);
        }

        #[test]
        fn adhoc_success_writes_current_result_without_touching_history_index() {
            let mut state = create_test_state();
            state.result_interaction.scroll_offset = 50;
            state.result_interaction.horizontal_offset = 10;
            state.result_interaction.enter_row(5);
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(2);
            let result = adhoc_result();

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.query.result_history.len(), 1);
            assert_eq!(state.query.history_index(), None);
            assert!(state.query.current_result().is_some());
            assert_eq!(
                state.query.current_result().unwrap().source,
                QuerySource::Adhoc,
            );
            assert_eq!(state.result_interaction.scroll_offset, 0);
            assert_eq!(state.result_interaction.horizontal_offset, 0);
            assert_eq!(state.result_interaction.selection().row(), None);
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn adhoc_error_preserves_current_result_and_view_state() {
            let mut state = create_test_state();
            state.query.set_current_result(preview_result(5));
            state.result_interaction.scroll_offset = 20;
            state.result_interaction.horizontal_offset = 5;
            state.result_interaction.enter_row(3);
            let result = adhoc_error_result();

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result,
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.query.result_history.is_empty());
            assert_eq!(state.query.history_index(), None);
            assert_eq!(
                state.query.current_result().unwrap().source,
                QuerySource::Preview,
            );
            assert_eq!(state.result_interaction.scroll_offset, 20);
            assert_eq!(state.result_interaction.horizontal_offset, 5);
            assert_eq!(state.result_interaction.selection().row(), Some(3));
        }

        #[test]
        fn preview_clears_history_index() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state.query.result_history.push(adhoc_result());
            state.query.enter_history(0);

            reduce_query(
                &mut state,
                &Action::QueryCompleted {
                    result: preview_result(5),
                    generation: 1,
                    target_page: Some(0),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.query.history_index(), None);
            assert!(state.query.current_result().is_some());
        }
    }

    mod query_failed {
        use super::*;
        use crate::app::ui_state::ResultNavMode;

        #[test]
        fn resets_result_selection_and_offsets() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state.result_interaction.enter_row(5);
            state.result_interaction.enter_cell(2);
            state.result_interaction.scroll_offset = 10;
            state.result_interaction.horizontal_offset = 3;

            reduce_query(
                &mut state,
                &Action::QueryFailed("error".to_string(), 1),
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.result_interaction.selection().mode(),
                ResultNavMode::Scroll
            );
            assert_eq!(state.result_interaction.scroll_offset, 0);
            assert_eq!(state.result_interaction.horizontal_offset, 0);
        }
    }

    mod adhoc_refresh {
        use super::*;
        use crate::domain::CommandTag;

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
            state.sql_modal.begin_prefetch();
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.users".to_string());
            state
                .session
                .set_table_detail_raw(Some(users_table_detail()));

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

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(state.sql_modal.prefetch_queue.is_empty());
            assert!(state.session.table_detail().is_none());
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
            let result = Arc::new(crate::domain::QueryResult {
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
        use crate::domain::{CommandTag, DatabaseMetadata, TableSummary};

        fn make_metadata(tables: Vec<(&str, &str)>) -> Arc<DatabaseMetadata> {
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables
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

            let stored = state.query.current_result().unwrap();
            assert_eq!(stored.source, QuerySource::Preview);
            assert_eq!(stored.row_count, 5);
        }

        #[test]
        fn ddl_create_then_metadata_loaded_preserves_explorer_selection() {
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

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let meta_effects = reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            )
            .unwrap();

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
            state.query.set_current_result(preview_result(3));

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

            let metadata = make_metadata(vec![("public", "orders")]);
            reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            );

            assert!(state.query.pagination.table.is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.table_detail().is_none());
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
                *state.sql_modal.status(),
                crate::app::sql_modal_context::SqlModalStatus::Success
            );
        }

        #[test]
        fn success_snapshot_not_overwritten_by_subsequent_preview_result() {
            let mut state = state_with_table("public", "users");

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
                .last_adhoc_success()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(saved_tag, Some(CommandTag::Alter(_))));

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

            let tag_after = state
                .sql_modal
                .last_adhoc_success()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(tag_after, Some(CommandTag::Alter(_))));
        }
    }
}
