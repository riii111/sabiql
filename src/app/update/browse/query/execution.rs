use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cmd::effect::Effect;
use crate::domain::{QueryResult, QuerySource};
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
use crate::model::shared::help::HelpOrigin;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::AdhocSuccessSnapshot;
use crate::services::AppServices;
use crate::update::action::{Action, ModalKind, TableTarget};
use crate::update::dispatch_result::DispatchResult;
use crate::update::input::command::{command_to_action, parse_command};

fn try_adhoc_refresh(state: &mut AppState, result: &QueryResult, now: Instant) -> Vec<Effect> {
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
        let run_id = state.session.begin_metadata_refresh();

        effects.push(Effect::CacheInvalidate { dsn: dsn.clone() });
        effects.push(Effect::ClearCompletionEngineCache);
        effects.push(Effect::FetchMetadata { dsn, run_id });
    } else if !state.query.pagination.table.is_empty() {
        let page = state.query.pagination.current_page;
        let run_id = state.query.begin_running(now);
        effects.push(Effect::ExecutePreview {
            dsn,
            schema: state.query.pagination.schema.clone(),
            table: state.query.pagination.table.clone(),
            generation: state.session.selection_generation(),
            run_id,
            limit: PREVIEW_PAGE_SIZE,
            offset: page * PREVIEW_PAGE_SIZE,
            target_page: page,
            read_only: state.session.read_only,
        });
    }

    effects
}

pub fn reduce_execution(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::QueryCompleted {
            dsn,
            run_id,
            result,
            generation,
            target_page,
        } => {
            if state.session.dsn.as_ref() != Some(dsn) || !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            if *generation == 0 || *generation == state.session.selection_generation() {
                state.query.mark_idle();
                let preserved_result_col = state.result_interaction.selection().cell();
                let preserved_horizontal_offset = state.result_interaction.horizontal_offset;

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
                            .finish_adhoc_error(result.error.clone().unwrap_or_default());
                    } else {
                        state.sql_modal.finish_adhoc_success(AdhocSuccessSnapshot {
                            command_tag: result.command_tag.clone(),
                            row_count: result.row_count,
                            execution_time_ms: result.execution_time_ms,
                        });
                    }
                }

                if result.source == QuerySource::Adhoc && !result.is_error() {
                    state.query.push_history(Arc::clone(result));
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
                            state.result_interaction.reset_interaction();
                        }
                        PostDeleteRowSelection::Select(row) => {
                            if !result.rows.is_empty() && !result.columns.is_empty() {
                                let clamped = row.min(result.rows.len() - 1);
                                let max_col = result.columns.len() - 1;
                                let col = preserved_result_col
                                    .unwrap_or(preserved_horizontal_offset)
                                    .min(max_col);
                                state.result_interaction.horizontal_offset =
                                    preserved_horizontal_offset.min(max_col).min(col);
                                state.result_interaction.activate_cell(clamped, col);

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

                DispatchResult::handled_with(try_adhoc_refresh(state, result, now))
            } else {
                DispatchResult::handled()
            }
        }
        Action::QueryFailed {
            dsn,
            run_id,
            error,
            generation,
            source,
        } => {
            if state.session.dsn.as_ref() != Some(dsn) || !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            if *generation == 0 || *generation == state.session.selection_generation() {
                state.query.mark_idle();
                if *source == QuerySource::Preview {
                    state.result_interaction.reset_view();
                    state
                        .query
                        .set_post_delete_selection(PostDeleteRowSelection::Keep);
                    state.query.clear_delete_refresh_target();
                    let preview_query = if state.query.pagination.schema.is_empty() {
                        state.query.pagination.table.clone()
                    } else {
                        format!(
                            "{}.{}",
                            state.query.pagination.schema, state.query.pagination.table
                        )
                    };
                    state.query.set_current_result(Arc::new(QueryResult::error(
                        preview_query,
                        error.result_message(),
                        0,
                        QuerySource::Preview,
                    )));
                } else {
                    let user_message = error.user_message();
                    state.set_error(user_message.clone());
                    state.sql_modal.finish_adhoc_error(user_message);
                }
            }
            DispatchResult::handled()
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(state.command_line_input.content());
            let follow_up = command_to_action(cmd);
            state.modal.pop_mode();
            state.command_line_input.clear();

            DispatchResult::handled_with(match follow_up {
                Action::Quit => {
                    state.should_quit = true;
                    vec![]
                }
                Action::ToggleModal(ModalKind::Help) => {
                    state.ui.help.open(HelpOrigin::CommandLine);
                    state.modal.push_mode(InputMode::Help);
                    vec![]
                }
                Action::OpenModal(ModalKind::SqlModal) => {
                    vec![Effect::DispatchActions(vec![Action::OpenModal(
                        ModalKind::SqlModal,
                    )])]
                }
                Action::OpenModal(ModalKind::ErTablePicker) => {
                    // Defer to modal reducer so metadata readiness checks stay in one place.
                    vec![Effect::DispatchActions(vec![Action::OpenModal(
                        ModalKind::ErTablePicker,
                    )])]
                }
                Action::OpenModal(ModalKind::Settings) => {
                    vec![Effect::DispatchActions(vec![Action::OpenModal(
                        ModalKind::Settings,
                    )])]
                }
                Action::OpenModal(ModalKind::CommandPalette) => {
                    vec![Effect::DispatchActions(vec![Action::OpenModal(
                        ModalKind::CommandPalette,
                    )])]
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
                let run_id = state.query.begin_running(now);

                state.query.pagination.reset();
                state.query.pagination.schema.clone_from(schema);
                state.query.pagination.table.clone_from(table);

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

                DispatchResult::handled_with(vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                    run_id,
                    limit: PREVIEW_PAGE_SIZE,
                    offset: 0,
                    target_page: 0,
                    read_only: state.session.read_only,
                }])
            } else {
                DispatchResult::handled()
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.session.dsn {
                let run_id = state.query.begin_running(now);
                DispatchResult::handled_with(vec![Effect::ExecuteAdhoc {
                    dsn: dsn.clone(),
                    run_id,
                    query: query.clone(),
                    read_only: state.session.read_only,
                }])
            } else {
                DispatchResult::handled()
            }
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::browse::query_execution::PaginationState;
    use crate::ports::outbound::DbOperationError;
    use crate::update::browse::query::dispatch_query;
    use crate::update::browse::query::tests::*;

    fn begin_query_run(state: &mut AppState) -> u64 {
        state.query.begin_running(Instant::now())
    }

    fn query_completed_action(
        state: &mut AppState,
        result: Arc<QueryResult>,
        generation: u64,
        target_page: Option<usize>,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::QueryCompleted {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            result,
            generation,
            target_page,
        }
    }

    fn query_failed_action(
        state: &mut AppState,
        error: DbOperationError,
        generation: u64,
        source: QuerySource,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::QueryFailed {
            dsn: "postgres://localhost/test".to_string(),
            run_id,
            error,
            generation,
            source,
        }
    }

    mod command_line_submit {
        use super::*;

        #[test]
        fn submit_quit_pops_mode_and_sets_quit() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("q".to_string());

            dispatch_query(
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
            state
                .command_line_input
                .set_content("unknown_cmd".to_string());

            dispatch_query(
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
            state.command_line_input.set_content("erd".to_string());

            let effects = dispatch_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.command_line_input.content().is_empty());
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::DispatchActions(actions) => {
                    assert!(matches!(
                        actions[0],
                        Action::OpenModal(ModalKind::ErTablePicker)
                    ));
                }
                other => panic!("expected DispatchActions, got {other:?}"),
            }
        }

        #[test]
        fn submit_settings_dispatches_open_settings() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("settings".to_string());

            let effects = dispatch_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.command_line_input.content().is_empty());
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::DispatchActions(actions) => {
                    assert!(matches!(actions[0], Action::OpenModal(ModalKind::Settings)));
                }
                other => panic!("expected DispatchActions, got {other:?}"),
            }
        }

        #[test]
        fn submit_palette_dispatches_open_command_palette() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("palette".to_string());

            let effects = dispatch_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.command_line_input.content().is_empty());
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::DispatchActions(actions) => {
                    assert!(matches!(
                        actions[0],
                        Action::OpenModal(ModalKind::CommandPalette)
                    ));
                }
                other => panic!("expected DispatchActions, got {other:?}"),
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

            dispatch_query(
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
            let action = query_completed_action(&mut state, result, 1, Some(2));

            dispatch_query(&mut state, &action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page, 2);
            assert!(state.query.pagination.reached_end);
        }

        #[test]
        fn does_not_set_reached_end_for_full_page() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let result = preview_result(PREVIEW_PAGE_SIZE);
            let now = Instant::now();
            let action = query_completed_action(&mut state, result, 1, Some(0));

            dispatch_query(&mut state, &action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page, 0);
            assert!(!state.query.pagination.reached_end);
        }

        #[test]
        fn adhoc_does_not_update_pagination() {
            let mut state = create_test_state();
            state.query.pagination.current_page = 3;
            let result = adhoc_result();
            let now = Instant::now();
            let action = query_completed_action(&mut state, result, 0, None);

            dispatch_query(&mut state, &action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page, 3);
        }

        #[test]
        fn adhoc_success_writes_current_result_without_touching_history_index() {
            let mut state = create_test_state();
            state.result_interaction.scroll_offset = 50;
            state.result_interaction.horizontal_offset = 10;
            state.result_interaction.activate_cell(5, 0);
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(2);
            let result = adhoc_result();
            let action = query_completed_action(&mut state, result, 0, None);

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(state.query.result_history().len(), 1);
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
            state.result_interaction.activate_cell(3, 0);
            let result = adhoc_error_result();
            let action = query_completed_action(&mut state, result, 0, None);

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert!(state.query.result_history().is_empty());
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
            state.query.push_history(adhoc_result());
            state.query.enter_history(0);
            let action = query_completed_action(&mut state, preview_result(5), 1, Some(0));

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(state.query.history_index(), None);
            assert!(state.query.current_result().is_some());
        }

        #[test]
        fn preview_delete_reselection_preserves_active_column_and_offset() {
            let mut state = create_test_state();
            let result = Arc::new(QueryResult {
                query: "SELECT * FROM users".to_string(),
                columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
                rows: vec![
                    vec![
                        "1".to_string(),
                        "Alice".to_string(),
                        "a@example.com".to_string(),
                    ],
                    vec![
                        "2".to_string(),
                        "Bob".to_string(),
                        "b@example.com".to_string(),
                    ],
                ],
                row_count: 2,
                execution_time_ms: 10,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            });
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Select(1));
            state.result_interaction.horizontal_offset = 1;
            state.result_interaction.activate_cell(3, 2);
            state.query.set_current_result(Arc::clone(&result));
            let action = query_completed_action(&mut state, result, 0, Some(0));

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(state.result_interaction.selection().row(), Some(1));
            assert_eq!(state.result_interaction.selection().cell(), Some(2));
            assert_eq!(state.result_interaction.horizontal_offset, 1);
        }

        #[test]
        fn preview_delete_clear_still_clears_staged_rows() {
            let mut state = create_test_state();
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Clear);
            state.result_interaction.activate_cell(0, 0);
            state.result_interaction.stage_row(0);
            let action = query_completed_action(&mut state, preview_result(1), 0, Some(0));

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(state.result_interaction.selection().row(), None);
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn stale_run_does_not_replace_current_result() {
            let mut state = create_test_state();
            let old_run_id = begin_query_run(&mut state);
            let _ = begin_query_run(&mut state);

            dispatch_query(
                &mut state,
                &Action::QueryCompleted {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: old_run_id,
                    result: adhoc_result(),
                    generation: 0,
                    target_page: None,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.query.current_result().is_none());
            assert!(state.query.is_running());
        }
    }

    mod query_failed {
        use super::*;
        use crate::model::shared::ui_state::ResultNavMode;
        use crate::ports::outbound::DbOperationError;

        #[test]
        fn resets_result_selection_and_offsets() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state.result_interaction.activate_cell(5, 2);
            state.result_interaction.scroll_offset = 10;
            state.result_interaction.horizontal_offset = 3;
            let action = query_failed_action(
                &mut state,
                DbOperationError::QueryFailed("error".to_string()),
                1,
                QuerySource::Preview,
            );

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(
                state.result_interaction.selection().mode(),
                ResultNavMode::Scroll
            );
            assert_eq!(state.result_interaction.scroll_offset, 0);
            assert_eq!(state.result_interaction.horizontal_offset, 0);
        }

        #[test]
        fn preview_failure_sets_error_result() {
            let mut state = state_with_table("public", "users");
            state.session.set_selection_generation(1);
            let action = query_failed_action(
                &mut state,
                DbOperationError::PermissionDenied("forbidden".to_string()),
                1,
                QuerySource::Preview,
            );

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let result = state.query.current_result().expect("result");
            assert!(result.is_error());
            assert_eq!(result.source, QuerySource::Preview);
            assert_eq!(result.query, "public.users");
            assert!(
                result
                    .error
                    .as_deref()
                    .is_some_and(|message| message.contains("Permission denied"))
            );
            assert!(state.messages.last_error.is_none());
        }

        #[test]
        fn preview_failure_does_not_become_adhoc_error_when_sql_modal_is_open() {
            let mut state = state_with_table("public", "users");
            state.session.set_selection_generation(1);
            state.modal.set_mode(InputMode::SqlModal);
            state
                .sql_modal
                .finish_adhoc_error("previous adhoc error".to_string());
            let action = query_failed_action(
                &mut state,
                DbOperationError::PermissionDenied("forbidden".to_string()),
                1,
                QuerySource::Preview,
            );

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert_eq!(
                state.sql_modal.last_adhoc_error(),
                Some("previous adhoc error")
            );
            let result = state.query.current_result().expect("result");
            assert_eq!(result.source, QuerySource::Preview);
            assert!(result.is_error());
        }
    }

    mod adhoc_refresh {
        use super::*;
        use crate::domain::CommandTag;

        #[test]
        fn dml_with_table_selected_emits_execute_preview() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Update(3)),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::ExecutePreview { table, .. } if table == "users")
            );
        }

        #[test]
        fn dml_without_table_selected_emits_no_effects() {
            let mut state = create_test_state();
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Insert(1)),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn ddl_emits_cache_invalidate_and_fetch_metadata() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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
            let _ = state.sql_modal.begin_prefetch();
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.users".to_string());
            state
                .session
                .set_table_detail_raw(Some(users_table_detail()));
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                0,
                None,
            );

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(state.sql_modal.prefetch_queue.is_empty());
            assert!(state.session.table_detail().is_none());
        }

        #[test]
        fn tcl_emits_no_effects() {
            for tag in [CommandTag::Begin, CommandTag::Commit, CommandTag::Rollback] {
                let mut state = state_with_table("public", "users");
                let action =
                    query_completed_action(&mut state, adhoc_result_with_tag(tag), 0, None);

                let effects =
                    dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub())
                        .unwrap();

                assert!(effects.is_empty());
            }
        }

        #[test]
        fn select_emits_no_effects() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Select(5)),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn adhoc_error_emits_no_effects() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(&mut state, adhoc_error_result(), 0, None);

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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
            let action = query_completed_action(&mut state, result, 0, None);

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod adhoc_refresh_integration {
        use super::*;
        use crate::domain::{CommandTag, DatabaseMetadata, TableSummary};
        use crate::update::browse::metadata::dispatch_metadata;

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
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Update(3)),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecutePreview { .. }));

            let new_preview = preview_result(5);
            let action = query_completed_action(&mut state, Arc::clone(&new_preview), 0, Some(0));
            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let stored = state.query.current_result().unwrap();
            assert_eq!(stored.source, QuerySource::Preview);
            assert_eq!(stored.row_count, 5);
        }

        #[test]
        fn ddl_create_then_metadata_loaded_preserves_explorer_selection() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let run_id = state.session.begin_metadata_refresh();
            let action = Action::MetadataLoaded {
                dsn: "postgres://localhost/test".to_string(),
                run_id,
                metadata,
            };
            let meta_effects = dispatch_metadata(&mut state, &action, Instant::now()).unwrap();

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
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            let metadata = make_metadata(vec![("public", "orders")]);
            let run_id = state.session.begin_metadata_refresh();
            let action = Action::MetadataLoaded {
                dsn: "postgres://localhost/test".to_string(),
                run_id,
                metadata,
            };
            dispatch_metadata(&mut state, &action, Instant::now());

            assert!(state.query.pagination.table.is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.table_detail().is_none());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn ddl_does_not_emit_execute_preview_so_modal_status_stays_success() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                0,
                None,
            );

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { .. }))
            );
            assert_eq!(
                *state.sql_modal.status(),
                crate::model::sql_editor::modal::SqlModalStatus::Success
            );
        }

        #[test]
        fn success_snapshot_not_overwritten_by_subsequent_preview_result() {
            let mut state = state_with_table("public", "users");
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Alter("TABLE".to_string())),
                0,
                None,
            );

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let saved_tag = state
                .sql_modal
                .last_adhoc_success()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(saved_tag, Some(CommandTag::Alter(_))));

            let action = query_completed_action(&mut state, preview_result(5), 0, Some(0));
            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let tag_after = state
                .sql_modal
                .last_adhoc_success()
                .and_then(|s| s.command_tag.clone());
            assert!(matches!(tag_after, Some(CommandTag::Alter(_))));
        }
    }
}
