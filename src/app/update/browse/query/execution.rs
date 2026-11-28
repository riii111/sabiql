use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cmd::effect::Effect;
use crate::domain::{QueryResult, QuerySource, RefreshScope};
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::{PREVIEW_PAGE_SIZE, PostDeleteRowSelection};
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::AdhocSuccessSnapshot;
use crate::ports::outbound::DbOperationError;
use crate::update::action::QueryContext;
use crate::update::browse::query::preview_effect_for_current_table;
use crate::update::helpers::fetch_metadata_effects;
use crate::update::input::keymap::{KeyContext, LineCommand, parse_command};
use crate::update::reducer::quit;
use crate::update::target_routing::open_modal;

use super::write;

pub(in crate::update) fn query_completed(
    state: &mut AppState,
    run_id: u64,
    result: Arc<QueryResult>,
    context: QueryContext,
    now: Instant,
) -> Vec<Effect> {
    if !state.query.is_current_run(run_id) {
        return vec![];
    }
    if let QueryContext::Preview { generation, .. } = context
        && generation != state.session.selection_generation()
    {
        return vec![];
    }

    state.query.mark_idle();

    match (result.source, result.is_error()) {
        // Adhoc errors stay inside the SQL modal; the existing preview
        // result and its view state are kept untouched.
        (QuerySource::Adhoc, true) => {
            state.sql_modal.finish_adhoc_error(
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| "Query failed".to_string()),
            );
        }
        (QuerySource::Adhoc, false) => {
            state.result_interaction.reset_view();
            state
                .query
                .set_result_highlight(now + Duration::from_millis(500));
            state.sql_modal.finish_adhoc_success(AdhocSuccessSnapshot {
                command_tag: result.command_tag.clone(),
                row_count: result.row_count(),
                execution_time_ms: result.execution_time_ms,
                mysql_diagnostics: result.mysql_diagnostics.clone(),
            });
            state.query.set_current_result(Arc::clone(&result));
        }
        // Preview errors arrive as error results and are shown in the
        // Result pane like any other preview.
        (QuerySource::Preview, _) => {
            let target_page = match context {
                QueryContext::Adhoc => None,
                QueryContext::Preview { target_page, .. } => Some(target_page),
            };
            apply_preview_result(state, &result, target_page, now);
        }
    }

    try_adhoc_refresh(state, &result, now)
}

pub(in crate::update) fn query_failed(
    state: &mut AppState,
    run_id: u64,
    error: DbOperationError,
    context: QueryContext,
    now: Instant,
) -> Vec<Effect> {
    if !state.query.is_current_run(run_id) {
        return vec![];
    }

    let is_preview = matches!(context, QueryContext::Preview { .. });
    if is_preview && matches!(error, DbOperationError::PreviewSizeExceeded(_)) {
        state.query.mark_idle();
        state.messages.set_error(error.user_message());
        return vec![];
    }

    if !matches!(
        context,
        QueryContext::Preview { generation, .. }
            if generation != state.session.selection_generation()
    ) {
        state.query.mark_idle();
        if is_preview {
            let result = Arc::new(preview_error_result(state, &error));
            state.result_interaction.reset_view();
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Keep);
            state.query.clear_delete_refresh_target();
            state.query.set_current_result(result);
        } else {
            let user_message = error.user_message();
            state.messages.set_error(user_message.clone());
            state.sql_modal.finish_adhoc_error(user_message);
        }
    }
    let refresh_scope = error
        .post_change_refresh_scope()
        .unwrap_or(RefreshScope::None);
    if is_preview {
        vec![]
    } else {
        refresh_effects_for_scope(state, refresh_scope, now)
    }
}

pub(in crate::update) fn command_line_submit(state: &mut AppState, now: Instant) -> Vec<Effect> {
    let command = parse_command(state.command_line_input.content());
    let context = KeyContext::from_state(state);
    state.modal.pop_mode();
    state.command_line_input.clear();

    match command {
        Some(LineCommand::Quit) => quit(state),
        Some(LineCommand::Help) => {
            state.ui.help_mut().open(context);
            state.modal.push_mode(InputMode::Help);
            vec![]
        }
        Some(LineCommand::Open(kind)) => open_modal(state, kind, now),
        Some(LineCommand::Write) => write::submit_cell_edit_write(state),
        None => vec![],
    }
}

pub(super) fn refresh_effects_for_scope(
    state: &mut AppState,
    refresh_scope: RefreshScope,
    now: Instant,
) -> Vec<Effect> {
    if refresh_scope == RefreshScope::None {
        return vec![];
    }
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };

    let mut effects = vec![];

    if refresh_scope == RefreshScope::Metadata {
        state.table_prefetch.reset_prefetch();
        state.er_preparation.invalidate_run();
        state.session.set_table_detail_raw(None);
        let run_id = state.session.begin_metadata_refresh();

        state.table_detail_cache.clear();

        effects.extend(fetch_metadata_effects(endpoint, run_id));
    } else if !state.query.pagination.table().is_empty() {
        let page = state.query.pagination.current_page();
        let generation = state.session.selection_generation();
        effects.extend(preview_effect_for_current_table(
            state, now, page, generation,
        ));
    }

    effects
}

fn try_adhoc_refresh(state: &mut AppState, result: &QueryResult, now: Instant) -> Vec<Effect> {
    if result.source != QuerySource::Adhoc || result.is_error() {
        return vec![];
    }
    refresh_effects_for_scope(state, result.refresh_scope, now)
}

fn preview_error_result(state: &AppState, error: &DbOperationError) -> QueryResult {
    QueryResult::error(
        state.query.pagination.qualified_name(),
        error.result_message(),
        0,
        QuerySource::Preview,
    )
}

fn apply_preview_result(
    state: &mut AppState,
    result: &Arc<QueryResult>,
    target_page: Option<usize>,
    now: Instant,
) {
    let preserved_result_col = state.result_interaction.selection().cell();
    let preserved_horizontal_offset = state.result_interaction.horizontal_offset();
    state.result_interaction.reset_view();
    state
        .query
        .set_result_highlight(now + Duration::from_millis(500));

    let should_apply_result = match target_page {
        Some(page)
            if !result.is_error()
                && result.data_row_count() == 0
                && page > state.query.pagination.current_page() =>
        {
            state.query.pagination.mark_reached_end();
            false
        }
        Some(page) => {
            state
                .query
                .pagination
                .set_page_result(page, result.data_row_count() < PREVIEW_PAGE_SIZE);
            true
        }
        None => true,
    };

    if should_apply_result {
        state.query.set_current_result(Arc::clone(result));
    }

    match state.query.post_delete_row_selection() {
        PostDeleteRowSelection::Keep => {}
        PostDeleteRowSelection::Clear => {
            state.result_interaction.reset_interaction();
        }
        PostDeleteRowSelection::Select(row) => {
            if result.data_row_count() > 0 && result.column_count() > 0 {
                let clamped = row.min(result.data_row_count() - 1);
                let max_col = result.column_count() - 1;
                let col = preserved_result_col
                    .unwrap_or(preserved_horizontal_offset)
                    .min(max_col);
                state
                    .result_interaction
                    .set_horizontal_offset(preserved_horizontal_offset.min(max_col).min(col));
                state.result_interaction.activate_cell(clamped, col);

                let visible = state.result_visible_rows();
                if visible > 0 && clamped >= visible {
                    state
                        .result_interaction
                        .set_scroll_offset(clamped - visible + 1);
                }
            }
        }
    }
    state
        .query
        .set_post_delete_selection(PostDeleteRowSelection::Keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::TableSignatureSnapshot;
    use crate::domain::{ConnectionId, DatabaseMetadata, DatabaseType};
    use crate::model::er_state::ErStatus;

    use crate::ports::outbound::DbOperationError;
    use crate::services::AppServices;
    use crate::test_support::endpoint;
    use crate::update::action::Action;
    use crate::update::action::SmartErRefreshSnapshot;
    use crate::update::browse::query::tests::*;
    use crate::update::reducer::{reduce, route};
    use crate::update::test_fixtures as update_test_fixtures;

    fn query_failed_action(
        state: &mut AppState,
        error: DbOperationError,
        generation: u64,
        source: QuerySource,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::QueryCompleted {
            run_id,
            context: match source {
                QuerySource::Adhoc => QueryContext::Adhoc,
                QuerySource::Preview => QueryContext::Preview {
                    generation,
                    target_page: 0,
                },
            },
            outcome: Err(error),
        }
    }

    fn state_with_selected_table(database_type: DatabaseType) -> (AppState, u64, u64) {
        let mut state = AppState::new("test".to_string());
        let dsn = match database_type {
            DatabaseType::PostgreSQL => "postgres://localhost/test",
            DatabaseType::MySQL => "mysql://localhost/test",
            DatabaseType::SQLite => "sqlite:///tmp/test.db",
        };
        state.session.activate_connection(
            &ConnectionId::new(),
            "test",
            &endpoint::for_engine(database_type, dsn),
        );
        let generation = state
            .session
            .select_table("public", "users", &mut state.query);
        let detail_run_id = state.session.begin_table_detail_run();
        (state, generation, detail_run_id)
    }

    mod command_line_submit {
        use super::*;

        #[test]
        fn submit_quit_pops_mode_and_sets_quit() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("q".to_string());
            let save_run_id = state.session.begin_connection_save();
            let probe_id = ConnectionId::from_string("pending-probe");
            let _ = state.session.begin_mysql_connection_probe(
                &probe_id,
                "mysql",
                &endpoint::mysql("mysql://localhost/app", Some("app")),
            );

            let effects = route(
                &mut state,
                Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.should_quit);
            assert!(!state.session.is_current_connection_save(save_run_id));
            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(matches!(effects.as_slice(), [Effect::CancelTrackedTasks]));
        }

        #[test]
        fn submit_unknown_pops_mode_without_side_effects() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::CellEdit);
            state.modal.push_mode(InputMode::CommandLine);
            state
                .command_line_input
                .set_content("unknown_cmd".to_string());

            route(
                &mut state,
                Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert!(!state.should_quit);
        }

        #[test]
        fn submit_write_enters_confirm_dialog_without_action_redispatch() {
            use crate::ports::inbound::{InputEvent, Key, KeyCombo};
            use crate::update::{input::handle_event, reducer::reduce};

            let mut state = create_test_state();
            state.query.set_current_result(editable_preview_result());
            state
                .session
                .set_table_detail_raw(Some(users_table_detail()));
            state.query.pagination.reset_for_table("public", "users");
            state.modal.set_mode(InputMode::CellEdit);
            state
                .result_interaction
                .begin_cell_edit(0, 1, "Alice".to_string());
            state
                .result_interaction
                .replace_cell_edit_draft("Bob".to_string());
            let services = AppServices::stub();
            for key in [Key::Char(':'), Key::Char('w')] {
                let action = handle_event(InputEvent::Key(KeyCombo::plain(key)), &state);
                reduce(&mut state, action, Instant::now(), &services);
            }
            let action = handle_event(InputEvent::Key(KeyCombo::plain(Key::Enter)), &state);
            let effects = reduce(&mut state, action, Instant::now(), &services);

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.result_interaction.pending_write_preview().is_some());
        }

        #[test]
        fn submit_erd_opens_er_table_picker_after_metadata() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("erd".to_string());

            let effects = route(
                &mut state,
                Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.command_line_input.content().is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.ui.pending_er_picker());
            assert!(effects.is_empty());
        }

        #[test]
        fn submit_settings_opens_settings() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("settings".to_string());

            let effects = route(
                &mut state,
                Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.command_line_input.content().is_empty());
            assert_eq!(state.input_mode(), InputMode::Settings);
            assert!(effects.is_empty());
        }

        #[test]
        fn submit_palette_opens_command_palette() {
            let mut state = create_test_state();
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("palette".to_string());

            let effects = route(
                &mut state,
                Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.command_line_input.content().is_empty());
            assert_eq!(state.input_mode(), InputMode::CommandPalette);
            assert!(effects.is_empty());
        }
    }

    mod preview_after_delete {
        use super::*;

        #[test]
        fn delete_success_then_adhoc_then_preview_completion_clears_selection() {
            let mut state = update_test_fixtures::state_after_delete_success();
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.editor.set_content("SELECT 1".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::SqlModalSubmit,
                now,
                &AppServices::stub(),
            );

            assert!(matches!(effects.as_slice(), [Effect::ExecuteAdhoc { .. }]));
            assert_eq!(
                state.query.post_delete_row_selection(),
                PostDeleteRowSelection::Keep
            );
            update_test_fixtures::complete_table_preview(&mut state, now);
            assert!(state.result_interaction.selection().row().is_none());
            assert!(state.result_interaction.selection().cell().is_none());
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

            route(&mut state, action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page(), 2);
            assert!(state.query.pagination.reached_end());
        }

        #[test]
        fn does_not_set_reached_end_for_full_page() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let result = preview_result(PREVIEW_PAGE_SIZE);
            let now = Instant::now();
            let action = query_completed_action(&mut state, result, 1, Some(0));

            route(&mut state, action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page(), 0);
            assert!(!state.query.pagination.reached_end());
        }

        #[test]
        fn applies_empty_initial_preview_at_page_zero() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let action = query_completed_action(&mut state, preview_result(0), 1, Some(0));

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(state.query.pagination.current_page(), 0);
            assert!(state.query.pagination.reached_end());
            assert_eq!(state.query.visible_result().unwrap().data_row_count(), 0);
        }

        #[test]
        fn applies_non_empty_forward_page() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let first_page = preview_result(PREVIEW_PAGE_SIZE);
            let first_action = query_completed_action(&mut state, first_page, 1, Some(0));
            route(
                &mut state,
                first_action,
                Instant::now(),
                &AppServices::stub(),
            );

            let next_action = query_completed_action(&mut state, preview_result(1), 1, Some(1));
            route(
                &mut state,
                next_action,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.query.pagination.current_page(), 1);
            assert!(state.query.pagination.reached_end());
            assert_eq!(state.query.visible_result().unwrap().data_row_count(), 1);
        }

        #[test]
        fn preserves_last_page_when_forward_preview_is_empty() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let last_page = preview_result(PREVIEW_PAGE_SIZE);
            let first_action =
                query_completed_action(&mut state, Arc::clone(&last_page), 1, Some(0));
            route(
                &mut state,
                first_action,
                Instant::now(),
                &AppServices::stub(),
            );

            let empty_action = query_completed_action(&mut state, preview_result(0), 1, Some(1));
            route(
                &mut state,
                empty_action,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.query.pagination.current_page(), 0);
            assert!(state.query.pagination.reached_end());
            assert!(Arc::ptr_eq(
                state.query.current_result().unwrap(),
                &last_page
            ));
            assert_eq!(
                state.query.visible_result().unwrap().data_row_count(),
                PREVIEW_PAGE_SIZE
            );
        }

        #[test]
        fn exact_page_boundaries_keep_next_available_until_empty() {
            for total_rows in [PREVIEW_PAGE_SIZE, PREVIEW_PAGE_SIZE * 2] {
                let mut state = create_test_state();
                state.session.set_selection_generation(1);
                let page_count = total_rows / PREVIEW_PAGE_SIZE;

                for page in 0..page_count {
                    let action = query_completed_action(
                        &mut state,
                        preview_result(PREVIEW_PAGE_SIZE),
                        1,
                        Some(page),
                    );
                    route(&mut state, action, Instant::now(), &AppServices::stub());
                    assert!(!state.query.pagination.reached_end());
                    assert!(state.query.pagination.can_next());
                }

                let empty_action =
                    query_completed_action(&mut state, preview_result(0), 1, Some(page_count));
                route(
                    &mut state,
                    empty_action,
                    Instant::now(),
                    &AppServices::stub(),
                );

                assert_eq!(state.query.pagination.current_page(), page_count - 1);
                assert!(state.query.pagination.reached_end());
                assert!(!state.query.pagination.can_next());
                assert_eq!(
                    state.query.visible_result().unwrap().data_row_count(),
                    PREVIEW_PAGE_SIZE
                );
            }
        }

        #[test]
        fn adhoc_does_not_update_pagination() {
            let mut state = create_test_state();
            state.query.pagination.set_page_result(3, false);
            let result = adhoc_result();
            let now = Instant::now();
            let action = query_completed_action(&mut state, result, 0, None);

            route(&mut state, action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page(), 3);
        }

        #[rstest::rstest]
        #[case(DatabaseType::PostgreSQL)]
        #[case(DatabaseType::MySQL)]
        #[case(DatabaseType::SQLite)]
        fn inspector_terminal_state_precedes_preview_for_all_engines(
            #[case] database_type: DatabaseType,
        ) {
            let (mut state, generation, detail_run_id) = state_with_selected_table(database_type);
            let now = Instant::now();
            let endpoint = active_endpoint(&state);

            let inspector_effects = route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint,
                    run_id: detail_run_id,
                    outcome: Ok(Box::new(users_table_detail())),
                    generation,
                },
                now,
                &AppServices::stub(),
            );

            assert!(inspector_effects.is_empty());
            assert!(state.session.is_table_detail_terminal(generation));
            assert!(state.query.current_result().is_none());

            let query_action =
                query_completed_action(&mut state, preview_result(1), generation, Some(0));
            route(&mut state, query_action, now, &AppServices::stub());

            assert!(state.query.current_result().is_some());
        }

        #[test]
        fn adhoc_success_writes_current_result_and_resets_view_state() {
            let mut state = create_test_state();
            state.result_interaction.set_scroll_offset(50);
            state.result_interaction.set_horizontal_offset(10);
            state.result_interaction.activate_cell(5, 0);
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(2);
            let result = adhoc_result();
            let action = query_completed_action(&mut state, result, 0, None);

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(state.query.current_result().is_some());
            assert_eq!(
                state.query.current_result().unwrap().source,
                QuerySource::Adhoc,
            );
            assert_eq!(state.result_interaction.scroll_offset(), 0);
            assert_eq!(state.result_interaction.horizontal_offset(), 0);
            assert_eq!(state.result_interaction.selection().row(), None);
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn adhoc_error_preserves_current_result_and_view_state() {
            let mut state = state_with_table("public", "users");
            state.query.set_current_result(preview_result(5));
            state.result_interaction.set_scroll_offset(20);
            state.result_interaction.set_horizontal_offset(5);
            state.result_interaction.activate_cell(3, 0);
            let result = adhoc_error_result();
            let action = query_completed_action(&mut state, result, 0, None);

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.is_empty());
            assert_eq!(
                state.query.current_result().unwrap().source,
                QuerySource::Preview,
            );
            assert_eq!(state.result_interaction.scroll_offset(), 20);
            assert_eq!(state.result_interaction.horizontal_offset(), 5);
            assert_eq!(state.result_interaction.selection().row(), Some(3));
        }

        #[test]
        fn preview_delete_reselection_preserves_active_column_and_offset() {
            let mut state = create_test_state();
            let result = Arc::new(QueryResult::success(
                "SELECT * FROM users".to_string(),
                vec!["id".to_string(), "name".to_string(), "email".to_string()],
                vec![
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
                10,
                QuerySource::Preview,
            ));
            state
                .query
                .set_post_delete_selection(PostDeleteRowSelection::Select(1));
            state.result_interaction.set_horizontal_offset(1);
            state.result_interaction.activate_cell(3, 2);
            state.query.set_current_result(Arc::clone(&result));
            let action = query_completed_action(&mut state, result, 0, Some(0));

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(state.result_interaction.selection().row(), Some(1));
            assert_eq!(state.result_interaction.selection().cell(), Some(2));
            assert_eq!(state.result_interaction.horizontal_offset(), 1);
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

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(state.result_interaction.selection().row(), None);
            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn stale_run_does_not_replace_current_result() {
            let mut state = create_test_state();
            let old_run_id = begin_query_run(&mut state);
            let _ = begin_query_run(&mut state);

            route(
                &mut state,
                Action::QueryCompleted {
                    run_id: old_run_id,
                    context: QueryContext::Adhoc,
                    outcome: Ok(adhoc_result()),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.query.current_result().is_none());
            assert!(state.query.is_running());
        }

        #[test]
        fn same_dsn_reconnect_rejects_previous_query_completion_by_run_id() {
            let mut state = create_test_state();
            let stale_run_id = begin_query_run(&mut state);

            state.session.reset(&mut state.query);
            state.session.activate_connection(
                &ConnectionId::new(),
                "postgres",
                &endpoint::postgres("postgres://localhost/test"),
            );
            let current_run_id = begin_query_run(&mut state);

            let action = Action::QueryCompleted {
                run_id: stale_run_id,
                context: QueryContext::Adhoc,
                outcome: Ok(adhoc_result()),
            };
            assert!(!format!("{action:?}").contains("dsn:"));
            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(stale_run_id < current_run_id);
            assert!(state.query.current_result().is_none());
            assert!(state.query.is_running());
        }

        #[test]
        fn selection_change_rejects_stale_adhoc_completion() {
            let mut state = create_test_state();
            let stale_run_id = begin_query_run(&mut state);
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);

            route(
                &mut state,
                Action::QueryCompleted {
                    run_id: stale_run_id,
                    context: QueryContext::Adhoc,
                    outcome: Ok(adhoc_result()),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.query.current_result().is_none());
            assert!(!state.query.is_running());
        }
    }

    mod query_failed {
        use super::*;
        use crate::model::shared::ui_state::ResultNavMode;
        use crate::model::sql_editor::modal::SqlModalStatus;

        #[test]
        fn resets_result_selection_and_offsets() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            state.result_interaction.activate_cell(5, 2);
            state.result_interaction.set_scroll_offset(10);
            state.result_interaction.set_horizontal_offset(3);
            let action = query_failed_action(
                &mut state,
                DbOperationError::QueryFailed("error".to_string()),
                1,
                QuerySource::Preview,
            );

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(
                state.result_interaction.selection().mode(),
                ResultNavMode::Scroll
            );
            assert_eq!(state.result_interaction.scroll_offset(), 0);
            assert_eq!(state.result_interaction.horizontal_offset(), 0);
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

            route(&mut state, action, Instant::now(), &AppServices::stub());

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
        fn size_failure_from_previous_table_preserves_current_preview_run() {
            let (mut state, generation, _) = state_with_selected_table(DatabaseType::MySQL);
            let failure = query_failed_action(
                &mut state,
                DbOperationError::PreviewSizeExceeded("field exceeded".into()),
                generation,
                QuerySource::Preview,
            );
            let _ = state
                .session
                .select_table("public", "orders", &mut state.query);
            let current_run = begin_query_run(&mut state);

            let effects = route(&mut state, failure, Instant::now(), &AppServices::stub());

            assert!(effects.is_empty());
            assert!(state.messages.last_error().is_none());
            assert!(state.query.is_current_run(current_run));
        }

        #[test]
        fn preview_size_failure_keeps_the_current_result_and_sets_an_error_message() {
            let mut state = state_with_table("public", "users");
            let current_result = preview_result(1);
            state.query.set_current_result(Arc::clone(&current_result));
            state.session.set_selection_generation(1);
            let action = query_failed_action(
                &mut state,
                DbOperationError::PreviewSizeExceeded("field exceeded".to_string()),
                1,
                QuerySource::Preview,
            );

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(
                state
                    .query
                    .current_result()
                    .is_some_and(|result| Arc::ptr_eq(result, &current_result))
            );
            assert_eq!(
                state.messages.last_error(),
                Some(
                    "Preview exceeded its byte budget: field exceeded. Reduce the preview value size and retry."
                )
            );
        }

        #[test]
        fn preview_failure_after_failed_inspector_shows_error_result() {
            let (mut state, generation, detail_run_id) =
                state_with_selected_table(DatabaseType::PostgreSQL);
            state.query.set_delete_refresh_target(1, None, 1);
            let query_action = query_failed_action(
                &mut state,
                DbOperationError::PermissionDenied("forbidden".to_string()),
                generation,
                QuerySource::Preview,
            );

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id: detail_run_id,
                    outcome: Err(DbOperationError::QueryFailed(
                        "inspector failed".to_string(),
                    )),
                    generation,
                },
                Instant::now(),
                &AppServices::stub(),
            );
            let effects = route(
                &mut state,
                query_action,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.messages.last_error().is_some());
            let result = state.query.current_result().expect("error result");
            assert!(result.is_error());
            assert!(state.query.pending_delete_refresh_target().is_none());
        }

        #[test]
        fn adhoc_failure_after_data_change_refreshes_preview() {
            let mut state = state_with_table("public", "users");
            let action = query_failed_action(
                &mut state,
                DbOperationError::QueryFailedAfterChange {
                    source: Arc::new(DbOperationError::QueryFailed(
                        "later statement failed".to_string(),
                    )),
                    refresh_scope: RefreshScope::Data,
                },
                0,
                QuerySource::Adhoc,
            );

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::ExecutePreview { table, .. } if table == "users"
            )));
            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Error(message) if message.contains("later statement failed")
            ));
        }

        #[test]
        fn adhoc_timeout_after_data_change_refreshes_preview() {
            let mut state = state_with_table("public", "users");
            let action = query_failed_action(
                &mut state,
                DbOperationError::QueryFailedAfterChange {
                    source: Arc::new(DbOperationError::Timeout(
                        "mysql query exceeded the execution timeout".to_string(),
                    )),
                    refresh_scope: RefreshScope::Data,
                },
                0,
                QuerySource::Adhoc,
            );

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::ExecutePreview { table, .. } if table == "users"
            )));
        }

        #[test]
        fn adhoc_failure_after_schema_change_refreshes_metadata() {
            let mut state = state_with_table("public", "users");
            let action = query_failed_action(
                &mut state,
                DbOperationError::QueryFailedAfterChange {
                    source: Arc::new(DbOperationError::QueryFailed(
                        "later DDL failed".to_string(),
                    )),
                    refresh_scope: RefreshScope::Metadata,
                },
                0,
                QuerySource::Adhoc,
            );

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(matches!(effects[0], Effect::CancelMetadataTasks));
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::ExecutePreview { .. }))
            );
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

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Error(message) if message == "previous adhoc error"
            ));
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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.is_empty());
        }

        #[test]
        fn ddl_metadata_refresh_invalidates_er_run_for_each_database() {
            for (database_type, dsn) in [
                (DatabaseType::PostgreSQL, "postgres://localhost/test"),
                (DatabaseType::MySQL, "mysql://localhost/test"),
                (DatabaseType::SQLite, "sqlite:///tmp/test.db"),
            ] {
                let mut state = AppState::new("test_project".to_string());
                state.session.activate_connection(
                    &ConnectionId::new(),
                    "test",
                    &endpoint::for_engine(database_type, dsn),
                );
                state.query.pagination.reset_for_table("public", "users");
                let er_run_id = state.er_preparation.start_waiting_run();
                let action = query_completed_action(
                    &mut state,
                    adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                    0,
                    None,
                );

                let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

                assert!(!state.er_preparation.is_current_run(er_run_id));
                assert!(effects.iter().any(|effect| matches!(
                    effect,
                    Effect::FetchMetadata { endpoint, .. }
                        if endpoint == &endpoint::for_engine(database_type, dsn)
                )));
            }
        }

        #[test]
        fn ddl_resets_prefetch_state_and_clears_table_detail() {
            let mut state = state_with_table("public", "users");
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
            state
                .session
                .set_table_detail_raw(Some(users_table_detail()));
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Drop("TABLE".to_string())),
                0,
                None,
            );

            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert!(!state.table_prefetch.has_pending_prefetch());
            assert!(state.session.table_detail().is_none());
        }

        #[test]
        fn ddl_metadata_refresh_rejects_old_er_completion() {
            let mut state = state_with_table("public", "users");
            let old_metadata = Arc::new(DatabaseMetadata::new("old".to_string()));
            state.session.set_metadata(Some(Arc::clone(&old_metadata)));
            let old_er_run_id = state.er_preparation.start_waiting_run();
            let action = query_completed_action(
                &mut state,
                adhoc_result_with_tag(CommandTag::Create("TABLE".to_string())),
                0,
                None,
            );

            route(&mut state, action, Instant::now(), &AppServices::stub());

            let endpoint = active_endpoint(&state);
            let effects = route(
                &mut state,
                Action::SmartErRefreshFetched {
                    endpoint,
                    run_id: old_er_run_id,
                    outcome: Ok(SmartErRefreshSnapshot {
                        new_metadata: Arc::new(DatabaseMetadata::new("new".to_string())),
                        signature_snapshot: Arc::new(TableSignatureSnapshot {
                            signatures: vec![],
                            prefetched_table_details: vec![],
                        }),
                    }),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert_eq!(
                state
                    .session
                    .metadata()
                    .map(|metadata| metadata.database_name.as_str()),
                Some("old")
            );
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
        }

        #[test]
        fn tcl_emits_no_effects() {
            for tag in [CommandTag::Begin, CommandTag::Commit, CommandTag::Rollback] {
                let mut state = state_with_table("public", "users");
                let action =
                    query_completed_action(&mut state, adhoc_result_with_tag(tag), 0, None);

                let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.is_empty());
        }

        #[test]
        fn no_command_tag_emits_no_effects() {
            let mut state = state_with_table("public", "users");
            let result = Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["?column?".to_string()],
                vec![vec!["1".to_string()]],
                5,
                QuerySource::Adhoc,
            ));
            let action = query_completed_action(&mut state, result, 0, None);

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(effects.is_empty());
        }
    }

    mod adhoc_refresh_integration {
        use super::*;
        use crate::domain::{CommandTag, TableSummary};
        use crate::model::sql_editor::modal::SqlModalStatus;

        fn make_metadata(tables: Vec<(&str, &str)>) -> Arc<DatabaseMetadata> {
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None)
                    })
                    .collect();
                metadata
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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecutePreview { .. }));

            let new_preview = preview_result(5);
            let action = query_completed_action(&mut state, Arc::clone(&new_preview), 0, Some(0));
            route(&mut state, action, Instant::now(), &AppServices::stub());

            let stored = state.query.current_result().unwrap();
            assert_eq!(stored.source, QuerySource::Preview);
            assert_eq!(stored.row_count(), 5);
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
            update_test_fixtures::seed_table_detail_cache(&mut state, "public.cached");

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(matches!(effects[0], Effect::CancelMetadataTasks));
            assert!(!state.table_detail_cache.contains("public.cached"));
            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
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

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let run_id = state.session.begin_metadata_refresh();
            let action = Action::MetadataLoaded {
                run_id,
                outcome: Ok(metadata),
            };
            let meta_effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(state.ui.explorer_selected(), 1);
            assert_eq!(state.query.pagination.table(), "users");
            assert!(
                meta_effects
                    .iter()
                    .any(|e| matches!(e, Effect::LoadTable { table, .. } if table == "users"))
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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );

            let metadata = make_metadata(vec![("public", "orders")]);
            let run_id = state.session.begin_metadata_refresh();
            let action = Action::MetadataLoaded {
                run_id,
                outcome: Ok(metadata),
            };
            route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(state.query.pagination.table().is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.table_detail().is_none());
            assert_eq!(state.ui.explorer_selected(), 0);
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

            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { .. }))
            );
            assert!(matches!(
                state.sql_modal.status(),
                SqlModalStatus::Success(_)
            ));
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

            route(&mut state, action, Instant::now(), &AppServices::stub());

            let saved_tag = match state.sql_modal.status() {
                SqlModalStatus::Success(snapshot) => snapshot.command_tag.clone(),
                _ => panic!("expected adhoc success status"),
            };
            assert!(matches!(saved_tag, Some(CommandTag::Alter(_))));

            let action = query_completed_action(&mut state, preview_result(5), 0, Some(0));
            route(&mut state, action, Instant::now(), &AppServices::stub());

            let tag_after = match state.sql_modal.status() {
                SqlModalStatus::Success(snapshot) => snapshot.command_tag.clone(),
                _ => panic!("expected adhoc success status after preview"),
            };
            assert!(matches!(tag_after, Some(CommandTag::Alter(_))));
        }
    }
}
