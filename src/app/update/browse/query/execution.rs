use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cmd::effect::Effect;
use crate::domain::{QueryResult, QuerySource, RefreshScope};
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::{
    PREVIEW_PAGE_SIZE, PendingPreview, PostDeleteRowSelection,
};
use crate::model::shared::help::HelpOrigin;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::AdhocSuccessSnapshot;
use crate::ports::outbound::{AccessMode, DbOperationError};
use crate::services::AppServices;
use crate::update::action::{
    Action, ModalKind, QueryCompletionContext, QueryFailureContext, TableTarget,
};
use crate::update::browse::query::preview_effect_for_current_table;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;
use crate::update::input::command::{command_to_action, parse_command};

use super::write;

pub fn reduce_execution(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::QueryCompleted {
            run_id,
            result,
            context,
        } => {
            if !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            if let QueryCompletionContext::Preview { generation, .. } = context
                && *generation != state.session.selection_generation()
            {
                return DispatchResult::handled();
            }

            if let QueryCompletionContext::Preview {
                generation,
                target_page,
            } = context
                && result.source == QuerySource::Preview
                && state.session.selected_table_key().is_some()
                && !state.session.is_table_detail_terminal(*generation)
            {
                state.query.mark_idle();
                state.query.defer_preview(
                    Arc::clone(result),
                    *generation,
                    Some(*target_page),
                    true,
                );
                return DispatchResult::handled();
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
                    reset_view_for_new_result(state, now);
                    state.sql_modal.finish_adhoc_success(AdhocSuccessSnapshot {
                        command_tag: result.command_tag.clone(),
                        row_count: result.row_count(),
                        execution_time_ms: result.execution_time_ms,
                        mysql_diagnostics: result.mysql_diagnostics.clone(),
                    });
                    state.query.set_current_result(Arc::clone(result));
                }
                // Preview errors arrive as error results and are shown in the
                // Result pane like any other preview.
                (QuerySource::Preview, _) => {
                    let target_page = match context {
                        QueryCompletionContext::Adhoc => None,
                        QueryCompletionContext::Preview { target_page, .. } => Some(*target_page),
                    };
                    apply_preview_result(state, result, target_page, now, true);
                }
            }

            DispatchResult::handled_with(try_adhoc_refresh(state, result, now))
        }
        Action::QueryFailed {
            run_id,
            error,
            context,
        } => {
            if !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            let is_preview = matches!(context, QueryFailureContext::Preview { .. });
            if is_preview && matches!(error, DbOperationError::PreviewSizeExceeded(_)) {
                state.query.mark_idle();
                state.messages.set_error(error.user_message());
                return DispatchResult::handled();
            }

            if let QueryFailureContext::Preview { generation } = context
                && *generation == state.session.selection_generation()
                && state.session.selected_table_key().is_some()
                && !state.session.is_table_detail_terminal(*generation)
            {
                let result = Arc::new(preview_error_result(state, error));
                state.query.mark_idle();
                state.query.defer_preview(result, *generation, None, false);
                return DispatchResult::handled();
            }

            if !matches!(
                context,
                QueryFailureContext::Preview { generation }
                    if *generation != state.session.selection_generation()
            ) {
                state.query.mark_idle();
                if is_preview {
                    let result = Arc::new(preview_error_result(state, error));
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
            let effects = if is_preview {
                vec![]
            } else {
                refresh_effects_for_scope(state, refresh_scope, now)
            };
            DispatchResult::handled_with(effects)
        }

        Action::RevealPendingPreview { generation } => {
            if !state.session.is_table_detail_terminal(*generation) {
                return DispatchResult::handled();
            }

            let Some(pending) = state.query.take_pending_preview(*generation) else {
                return DispatchResult::handled();
            };
            let PendingPreview {
                result,
                generation: _,
                target_page,
                highlight,
            } = pending;
            if result.is_error() && !highlight {
                state.query.clear_delete_refresh_target();
            }
            apply_preview_result(state, &result, target_page, now, highlight);
            DispatchResult::handled()
        }

        Action::CommandLineSubmit => {
            let cmd = parse_command(state.command_line_input.content());
            let follow_up = command_to_action(cmd);
            state.modal.pop_mode();
            state.command_line_input.clear();

            DispatchResult::handled_with(match follow_up {
                Action::Quit => {
                    state.session.cancel_connection_save_and_disconnect();
                    state.session.clear_mysql_connection_probe();
                    state.should_quit = true;
                    vec![Effect::CancelTrackedTasks]
                }
                Action::ToggleModal(ModalKind::Help) => {
                    state.ui.help_mut().open(HelpOrigin::CommandLine);
                    state.modal.push_mode(InputMode::Help);
                    vec![]
                }
                Action::OpenModal(
                    modal @ (ModalKind::SqlModal
                    | ModalKind::ErTablePicker
                    | ModalKind::Settings
                    | ModalKind::CommandPalette),
                ) => {
                    vec![Effect::DispatchActions(vec![Action::OpenModal(modal)])]
                }
                Action::SubmitCellEditWrite => {
                    write::reduce_write(state, &Action::SubmitCellEditWrite, now, services)
                        .into_effects()
                        .unwrap_or_default()
                }
                _ => vec![],
            })
        }

        Action::ExecutePreview(TableTarget {
            schema,
            table,
            generation,
        }) => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if state.session.dsn().is_none() {
                return DispatchResult::handled();
            }

            state.query.pagination.reset_for_table(schema, table);

            // Keep the generation captured at selection time, not the current
            // one: the selection may have been cleared between dispatch and
            // now, and such a completion must fail the stale check.
            match preview_effect_for_current_table(state, now, 0, *generation) {
                Some(effect) => DispatchResult::handled_with(vec![effect]),
                None => DispatchResult::handled(),
            }
        }

        Action::ExecuteAdhoc(query) => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if let Some(dsn) = state.session.dsn().map(String::from) {
                let run_id = state.query.begin_non_preview_running(now);
                DispatchResult::handled_with(vec![Effect::ExecuteAdhoc {
                    dsn,
                    run_id,
                    query: query.clone(),
                    access_mode: AccessMode::from_read_only(state.session.is_read_only()),
                }])
            } else {
                DispatchResult::handled()
            }
        }

        _ => DispatchResult::pass(),
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
    let Some(dsn) = state.session.dsn().map(String::from) else {
        return vec![];
    };

    let mut effects = vec![];

    if refresh_scope == RefreshScope::Metadata {
        state.sql_modal.reset_prefetch();
        state.session.set_table_detail_raw(None);
        let run_id = state.session.begin_metadata_refresh();

        effects.push(Effect::CancelMetadataTasks);
        effects.push(Effect::CacheInvalidate { dsn: dsn.clone() });
        effects.push(Effect::ClearCompletionEngineCache);
        effects.push(Effect::FetchMetadata { dsn, run_id });
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

fn reset_view_for_new_result(state: &mut AppState, now: Instant) {
    state.result_interaction.reset_view();
    state
        .query
        .set_result_highlight(now + Duration::from_millis(500));
}

fn apply_preview_result(
    state: &mut AppState,
    result: &Arc<QueryResult>,
    target_page: Option<usize>,
    now: Instant,
    highlight: bool,
) {
    let preserved_result_col = state.result_interaction.selection().cell();
    let preserved_horizontal_offset = state.result_interaction.horizontal_offset();
    state.result_interaction.reset_view();
    if highlight {
        state
            .query
            .set_result_highlight(now + Duration::from_millis(500));
    }

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
    use crate::cmd::cache::TtlCache;
    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::runner::EffectRunner;
    use crate::cmd::test_fixtures;
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{DbOperationError, RenderOutput, RenderResult, Renderer};
    use crate::update::browse::metadata::dispatch_metadata;
    use crate::update::browse::query::dispatch_query;
    use crate::update::browse::query::tests::*;
    use crate::update::reducer::reduce;
    use crate::update::test_fixtures as update_test_fixtures;
    use tokio::sync::mpsc;

    #[derive(Debug, PartialEq)]
    struct RenderFrame {
        inspector_terminal: bool,
        result_visible: bool,
    }

    struct RecordingRenderer {
        frames: Vec<RenderFrame>,
    }

    impl Renderer for RecordingRenderer {
        fn draw(
            &mut self,
            state: &AppState,
            _services: &AppServices,
            _now: Instant,
        ) -> RenderResult<RenderOutput> {
            self.frames.push(RenderFrame {
                inspector_terminal: state
                    .session
                    .is_table_detail_terminal(state.session.selection_generation()),
                result_visible: state.query.current_result().is_some(),
            });
            Ok(RenderOutput::default())
        }
    }

    fn append_runtime_render(state: &AppState, effects: &mut Vec<Effect>) {
        if state.render_dirty {
            effects.push(Effect::Render);
        }
    }

    async fn run_effects_and_clear_dirty<T: Renderer>(
        runner: &EffectRunner,
        effects: Vec<Effect>,
        renderer: &mut T,
        state: &mut AppState,
        completion_engine: &std::cell::RefCell<CompletionEngine>,
    ) -> Vec<Action> {
        let pending = runner
            .execute_effects(
                effects,
                renderer,
                state,
                completion_engine,
                &AppServices::stub(),
            )
            .await
            .unwrap();
        state.clear_dirty();
        pending
    }

    async fn render_frames_after_inspector_terminal(inspector_failed: bool) -> Vec<RenderFrame> {
        let (mut state, generation, detail_run_id) =
            state_with_selected_table(DatabaseType::PostgreSQL);
        let (tx, _rx) = mpsc::channel(8);
        let runner = test_fixtures::make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(MockQueryExecutor::new()),
            Arc::new(MockConnectionStore::new()),
            TtlCache::new(300),
            tx,
        );
        let completion_engine = std::cell::RefCell::new(CompletionEngine::new());
        let mut renderer = RecordingRenderer { frames: vec![] };
        let query_action =
            query_completed_action(&mut state, preview_result(1), generation, Some(0));
        let mut query_effects = reduce(
            &mut state,
            query_action,
            Instant::now(),
            &AppServices::stub(),
        );
        append_runtime_render(&state, &mut query_effects);
        assert!(
            run_effects_and_clear_dirty(
                &runner,
                query_effects,
                &mut renderer,
                &mut state,
                &completion_engine,
            )
            .await
            .is_empty()
        );
        assert_eq!(
            renderer.frames,
            [RenderFrame {
                inspector_terminal: false,
                result_visible: false,
            }]
        );
        renderer.frames.clear();

        let inspector_action = if inspector_failed {
            Action::TableDetailFailed {
                dsn: active_dsn(&state),
                run_id: detail_run_id,
                error: DbOperationError::QueryFailed("inspector failed".to_string()),
                generation,
            }
        } else {
            Action::TableDetailLoaded {
                dsn: active_dsn(&state),
                run_id: detail_run_id,
                detail: Box::new(users_table_detail()),
                generation,
            }
        };
        let mut effects = reduce(
            &mut state,
            inspector_action,
            Instant::now(),
            &AppServices::stub(),
        );
        append_runtime_render(&state, &mut effects);
        let pending = run_effects_and_clear_dirty(
            &runner,
            effects,
            &mut renderer,
            &mut state,
            &completion_engine,
        )
        .await;
        assert_eq!(
            renderer.frames,
            [RenderFrame {
                inspector_terminal: true,
                result_visible: false,
            }]
        );

        let mut next_effects = Vec::new();
        for action in pending {
            next_effects.extend(reduce(
                &mut state,
                action,
                Instant::now(),
                &AppServices::stub(),
            ));
        }
        append_runtime_render(&state, &mut next_effects);
        assert!(
            run_effects_and_clear_dirty(
                &runner,
                next_effects,
                &mut renderer,
                &mut state,
                &completion_engine,
            )
            .await
            .is_empty()
        );

        renderer.frames
    }

    fn query_failed_action(
        state: &mut AppState,
        error: DbOperationError,
        generation: u64,
        source: QuerySource,
    ) -> Action {
        let run_id = begin_query_run(state);
        Action::QueryFailed {
            run_id,
            error,
            context: match source {
                QuerySource::Adhoc => QueryFailureContext::Adhoc,
                QuerySource::Preview => QueryFailureContext::Preview { generation },
            },
        }
    }

    fn state_with_selected_table(database_type: DatabaseType) -> (AppState, u64, u64) {
        let mut state = AppState::new("test".to_string());
        let dsn = match database_type {
            DatabaseType::PostgreSQL => "postgres://localhost/test",
            DatabaseType::MySQL => "mysql://localhost/test",
            DatabaseType::SQLite => "sqlite:///tmp/test.db",
        };
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "test",
            database_type,
            dsn,
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
                "mysql://localhost/app",
                Some("app"),
            );

            let effects = dispatch_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

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
        fn submit_write_enters_confirm_dialog_without_action_redispatch() {
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
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.set_content("write".to_string());

            let effects = dispatch_query(
                &mut state,
                &Action::CommandLineSubmit,
                Instant::now(),
                &AppServices::stub(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.result_interaction.pending_write_preview().is_some());
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
            state
                .query
                .pagination
                .reset_for_table("old_schema", "old_table");
            state.query.pagination.set_page_result(5, true);
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

            assert_eq!(state.query.pagination.current_page(), 0);
            assert!(!state.query.pagination.reached_end());
            assert_eq!(state.query.pagination.schema(), "public");
            assert_eq!(state.query.pagination.table(), "users");
        }

        #[test]
        fn delete_success_then_adhoc_then_preview_completion_clears_selection() {
            let mut state = update_test_fixtures::state_after_delete_success();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ExecuteAdhoc("SELECT 1".to_string()),
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

        #[test]
        fn pending_probe_blocks_preview_and_adhoc_on_old_connection() {
            let mut state = create_test_state();
            let _ = state.session.begin_mysql_connection_probe(
                &ConnectionId::new(),
                "target",
                "mysql://target",
                Some("app"),
            );

            let preview_effects = dispatch_query(
                &mut state,
                &Action::ExecutePreview(TableTarget {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                }),
                Instant::now(),
                &AppServices::stub(),
            )
            .into_effects()
            .expect("preview should be handled");
            let adhoc_effects = dispatch_query(
                &mut state,
                &Action::ExecuteAdhoc("SELECT 1".to_string()),
                Instant::now(),
                &AppServices::stub(),
            )
            .into_effects()
            .expect("adhoc should be handled");

            assert!(preview_effects.is_empty());
            assert!(adhoc_effects.is_empty());
            assert!(!state.query.is_running());
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

            dispatch_query(&mut state, &action, now, &AppServices::stub());

            assert_eq!(state.query.pagination.current_page(), 0);
            assert!(!state.query.pagination.reached_end());
        }

        #[test]
        fn applies_empty_initial_preview_at_page_zero() {
            let mut state = create_test_state();
            state.session.set_selection_generation(1);
            let action = query_completed_action(&mut state, preview_result(0), 1, Some(0));

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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
            dispatch_query(
                &mut state,
                &first_action,
                Instant::now(),
                &AppServices::stub(),
            );

            let next_action = query_completed_action(&mut state, preview_result(1), 1, Some(1));
            dispatch_query(
                &mut state,
                &next_action,
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
            dispatch_query(
                &mut state,
                &first_action,
                Instant::now(),
                &AppServices::stub(),
            );

            let empty_action = query_completed_action(&mut state, preview_result(0), 1, Some(1));
            dispatch_query(
                &mut state,
                &empty_action,
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
                    dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());
                    assert!(!state.query.pagination.reached_end());
                    assert!(state.query.pagination.can_next());
                }

                let empty_action =
                    query_completed_action(&mut state, preview_result(0), 1, Some(page_count));
                dispatch_query(
                    &mut state,
                    &empty_action,
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
            state.query.pagination.set_current_page(3);
            let result = adhoc_result();
            let now = Instant::now();
            let action = query_completed_action(&mut state, result, 0, None);

            dispatch_query(&mut state, &action, now, &AppServices::stub());

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
            let dsn = active_dsn(&state);

            let inspector_effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn,
                    run_id: detail_run_id,
                    detail: Box::new(users_table_detail()),
                    generation,
                },
                now,
            )
            .unwrap();

            assert!(inspector_effects.is_empty());
            assert!(state.session.is_table_detail_terminal(generation));
            assert!(state.query.current_result().is_none());

            let query_action =
                query_completed_action(&mut state, preview_result(1), generation, Some(0));
            dispatch_query(&mut state, &query_action, now, &AppServices::stub());

            assert!(state.query.current_result().is_some());
            assert!(!state.query.has_pending_preview(generation));
        }

        #[tokio::test]
        async fn preview_reveal_follows_inspector_render_for_success_and_failure() {
            for inspector_failed in [false, true] {
                assert_eq!(
                    render_frames_after_inspector_terminal(inspector_failed).await,
                    [
                        RenderFrame {
                            inspector_terminal: true,
                            result_visible: false,
                        },
                        RenderFrame {
                            inspector_terminal: true,
                            result_visible: true,
                        },
                    ]
                );
            }
        }

        #[test]
        fn later_adhoc_run_drops_pending_preview_before_inspector_finishes() {
            let (mut state, generation, detail_run_id) =
                state_with_selected_table(DatabaseType::PostgreSQL);
            let preview_action =
                query_completed_action(&mut state, preview_result(1), generation, Some(0));

            dispatch_query(
                &mut state,
                &preview_action,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.has_pending_preview(generation));

            let adhoc_action = query_completed_action(&mut state, adhoc_result(), 0, None);
            dispatch_query(
                &mut state,
                &adhoc_action,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.query.current_result().map(|result| result.source),
                Some(QuerySource::Adhoc)
            );
            assert!(!state.query.has_pending_preview(generation));

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: detail_run_id,
                    detail: Box::new(users_table_detail()),
                    generation,
                },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state.query.current_result().map(|result| result.source),
                Some(QuerySource::Adhoc)
            );
        }

        #[test]
        fn stale_selection_drops_old_pending_preview_before_new_one_is_released() {
            let (mut state, old_generation, old_detail_run_id) =
                state_with_selected_table(DatabaseType::PostgreSQL);
            let old_query_action =
                query_completed_action(&mut state, preview_result(1), old_generation, Some(0));
            dispatch_query(
                &mut state,
                &old_query_action,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.has_pending_preview(old_generation));

            let new_generation = state
                .session
                .select_table("public", "orders", &mut state.query);
            let new_detail_run_id = state.session.begin_table_detail_run();

            assert_ne!(old_generation, new_generation);
            assert!(!state.query.has_pending_preview(old_generation));
            assert!(state.query.current_result().is_none());

            dispatch_query(
                &mut state,
                &old_query_action,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.current_result().is_none());

            let stale_inspector_effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: old_detail_run_id,
                    detail: Box::new(users_table_detail()),
                    generation: old_generation,
                },
                Instant::now(),
            )
            .unwrap();
            assert!(stale_inspector_effects.is_empty());
            assert!(!state.session.is_table_detail_terminal(new_generation));

            let new_query_action =
                query_completed_action(&mut state, preview_result(1), new_generation, Some(0));
            dispatch_query(
                &mut state,
                &new_query_action,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.current_result().is_none());
            assert!(state.query.has_pending_preview(new_generation));

            dispatch_query(
                &mut state,
                &Action::RevealPendingPreview {
                    generation: old_generation,
                },
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.current_result().is_none());
            assert!(state.query.has_pending_preview(new_generation));

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: new_detail_run_id,
                    detail: Box::new(users_table_detail()),
                    generation: new_generation,
                },
                Instant::now(),
            )
            .unwrap();
            assert!(matches!(effects.as_slice(), [Effect::DispatchActions(_)]));

            dispatch_query(
                &mut state,
                &Action::RevealPendingPreview {
                    generation: new_generation,
                },
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.query.current_result().is_some());
            assert!(!state.query.has_pending_preview(new_generation));
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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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
            let mut state = create_test_state();
            state.query.set_current_result(preview_result(5));
            state.result_interaction.set_scroll_offset(20);
            state.result_interaction.set_horizontal_offset(5);
            state.result_interaction.activate_cell(3, 0);
            let result = adhoc_error_result();
            let action = query_completed_action(&mut state, result, 0, None);

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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
                    run_id: old_run_id,
                    result: adhoc_result(),
                    context: QueryCompletionContext::Adhoc,
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
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "postgres",
                DatabaseType::PostgreSQL,
                "postgres://localhost/test",
            );
            let current_run_id = begin_query_run(&mut state);

            let action = Action::QueryCompleted {
                run_id: stale_run_id,
                result: adhoc_result(),
                context: QueryCompletionContext::Adhoc,
            };
            assert!(!format!("{action:?}").contains("dsn:"));
            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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

            dispatch_query(
                &mut state,
                &Action::QueryCompleted {
                    run_id: stale_run_id,
                    result: adhoc_result(),
                    context: QueryCompletionContext::Adhoc,
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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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
        fn preview_failure_waits_for_inspector_then_releases_error_result() {
            let (mut state, generation, detail_run_id) =
                state_with_selected_table(DatabaseType::PostgreSQL);
            state.query.set_delete_refresh_target(1, None, 1);
            let query_action = query_failed_action(
                &mut state,
                DbOperationError::PermissionDenied("forbidden".to_string()),
                generation,
                QuerySource::Preview,
            );

            dispatch_query(
                &mut state,
                &query_action,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.query.current_result().is_none());
            assert!(state.query.has_pending_preview(generation));

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: detail_run_id,
                    error: DbOperationError::QueryFailed("inspector failed".to_string()),
                    generation,
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.is_table_detail_terminal(generation));
            assert!(state.messages.last_error().is_some());
            assert!(state.query.current_result().is_none());
            assert!(matches!(
                effects.as_slice(),
                [Effect::DispatchActions(actions)]
                    if matches!(
                        actions.as_slice(),
                        [Action::RevealPendingPreview { generation: action_generation }]
                            if *action_generation == generation
                    )
            ));

            dispatch_query(
                &mut state,
                &Action::RevealPendingPreview { generation },
                Instant::now(),
                &AppServices::stub(),
            );

            let result = state.query.current_result().expect("released result");
            assert!(result.is_error());
            assert!(state.query.pending_delete_refresh_target().is_none());
            assert!(!state.query.has_pending_preview(generation));
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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(matches!(effects[0], Effect::CancelMetadataTasks));
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CacheInvalidate { .. }))
            );
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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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

            assert!(matches!(effects[0], Effect::CancelMetadataTasks));
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
            let _ = state.sql_modal.begin_er_prefetch();
            state
                .sql_modal
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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            assert!(state.sql_modal.active_prefetch_run_id().is_none());
            assert!(!state.sql_modal.has_pending_prefetch());
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
            let result = Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["?column?".to_string()],
                vec![vec!["1".to_string()]],
                5,
                QuerySource::Adhoc,
            ));
            let action = query_completed_action(&mut state, result, 0, None);

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod adhoc_refresh_integration {
        use super::*;
        use crate::domain::{CommandTag, DatabaseMetadata, TableSummary};
        use crate::model::sql_editor::modal::SqlModalStatus;

        fn make_metadata(tables: Vec<(&str, &str)>) -> Arc<DatabaseMetadata> {
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None, false)
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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::ExecutePreview { .. }));

            let new_preview = preview_result(5);
            let action = query_completed_action(&mut state, Arc::clone(&new_preview), 0, Some(0));
            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

            assert!(state.sql_modal.active_prefetch_run_id().is_none());
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

            assert_eq!(state.ui.explorer_selected(), 1);
            assert_eq!(state.query.pagination.table(), "users");
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

            let effects =
                dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub()).unwrap();

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

            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let saved_tag = match state.sql_modal.status() {
                SqlModalStatus::Success(snapshot) => snapshot.command_tag.clone(),
                _ => panic!("expected adhoc success status"),
            };
            assert!(matches!(saved_tag, Some(CommandTag::Alter(_))));

            let action = query_completed_action(&mut state, preview_result(5), 0, Some(0));
            dispatch_query(&mut state, &action, Instant::now(), &AppServices::stub());

            let tag_after = match state.sql_modal.status() {
                SqlModalStatus::Success(snapshot) => snapshot.command_tag.clone(),
                _ => panic!("expected adhoc success status after preview"),
            };
            assert!(matches!(tag_after, Some(CommandTag::Alter(_))));
        }
    }
}
