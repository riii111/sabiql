mod er_neighbors;
pub(in crate::update) mod loading;
pub(in crate::update) mod prefetch;
pub(in crate::update) mod table_detail;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::model::shared::detail_view::ReadOnlyDetailState;
use crate::model::shared::input_mode::InputMode;
use crate::update::er::diagram::er_generate_from_cache;

pub(super) fn check_er_completion(state: &mut AppState) -> Vec<Effect> {
    if state.er_preparation.status() != ErStatus::Waiting || !state.table_prefetch.is_complete() {
        return vec![];
    }

    if !state.er_preparation.fk_expanded() {
        let Some(run_id) = state.table_prefetch.active_prefetch_run_id() else {
            return vec![];
        };
        return er_neighbors::expand_prefetch_with_fk_neighbors(state, run_id);
    }

    if !state.table_prefetch.has_failures() {
        state.er_preparation.mark_idle();
        return er_generate_from_cache(state);
    }

    state.er_preparation.mark_idle();
    let mut failed_tables = state.table_prefetch.failed_table_errors();
    failed_tables.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let names = failed_tables
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    state
        .messages
        .set_error("ER failed. Close details and press 'e' to retry.".to_string());
    let content = format!("Close this view and press 'e' to retry.\n\nFailed tables:\n{names}");
    let detail =
        ReadOnlyDetailState::open(0, 0, "ER failures".to_string(), content.clone(), content);
    state.er_failure_previous_detail = Some(std::mem::replace(&mut state.cell_detail, detail));
    state.modal.push_mode(InputMode::CellDetail);
    vec![]
}

#[cfg(test)]
mod tests {
    use crate::test_support;
    use crate::test_support::endpoint;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::domain::{ConnectionId, Table};
    use crate::model::app_state::AppState;
    use crate::model::browse::session::TableDetailState;
    use crate::model::shared::input_mode::InputMode;
    use crate::ports::outbound::DbOperationError;
    use crate::update::action::{Action, ModalKind};
    use crate::update::reducer::route;
    use std::sync::Arc;
    use std::time::Instant;

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::new(),
            "postgres",
            &endpoint::postgres(dsn),
        );
        state
    }

    fn sqlite_state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state
            .session
            .activate_connection(&ConnectionId::new(), "sqlite", &endpoint::sqlite(dsn));
        state
    }

    fn empty_table(schema: &str, name: &str) -> Box<Table> {
        Box::new(test_support::table::minimal(schema, name))
    }

    fn cache_keeps_every_table(state: &mut AppState, table_count: usize) -> bool {
        for index in 0..table_count {
            state
                .table_detail_cache
                .insert(format!("public.t{index}"), *empty_table("public", "t"));
        }
        state.table_detail_cache.contains("public.t0")
    }

    #[test]
    fn er_failure_shows_failed_table_names_and_retains_them_for_retry() {
        let mut state = state_with_dsn("postgres://localhost/test");
        let _ = state.er_preparation.start_waiting_run();
        state.er_preparation.mark_fk_expanded();
        state
            .table_prefetch
            .fail("public.users".into(), "permission denied".into());
        state
            .table_prefetch
            .fail("public.orders".into(), "connection failed".into());

        assert!(check_er_completion(&mut state).is_empty());
        let message = state.messages.last_error().unwrap();
        assert!(message.contains("'e' to retry"));
        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert!(
            state
                .cell_detail
                .content()
                .contains("public.orders\npublic.users")
        );
        assert!(state.table_prefetch.failed_prefetch("public.users"));
        assert!(state.table_prefetch.failed_prefetch("public.orders"));
        assert_eq!(state.er_preparation.status(), ErStatus::Idle);
        route(
            &mut state,
            Action::CloseModal(ModalKind::CellDetail),
            Instant::now(),
        );
        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(state.er_failure_previous_detail.is_none());
    }

    #[test]
    fn er_failure_details_return_to_an_existing_cell_detail() {
        let mut state = state_with_dsn("postgres://localhost/test");
        state.cell_detail =
            ReadOnlyDetailState::open(1, 2, "name".into(), "before".into(), "before".into());
        state.modal.push_mode(InputMode::CellDetail);
        let _ = state.er_preparation.start_waiting_run();
        state.er_preparation.mark_fk_expanded();
        state
            .table_prefetch
            .fail("public.users".into(), "denied".into());
        check_er_completion(&mut state);
        route(
            &mut state,
            Action::CloseModal(ModalKind::CellDetail),
            Instant::now(),
        );
        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(state.cell_detail.content(), "before");
        assert!(state.cell_detail.is_active());
    }

    mod freshness_guards {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};
        use crate::model::connection::state::ConnectionState;

        fn metadata_with_users() -> Arc<DatabaseMetadata> {
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                )];
                metadata
            })
        }

        #[test]
        fn stale_metadata_loaded_does_not_replace_current_state() {
            let mut state = state_with_dsn("postgres://localhost/new");
            let stale_run_id = state.session.begin_metadata_refresh();
            let _current_run_id = state.session.begin_metadata_refresh();

            let effects = route(
                &mut state,
                Action::MetadataLoaded {
                    run_id: stale_run_id,
                    outcome: Ok(metadata_with_users()),
                },
                Instant::now(),
            );

            assert!(effects.is_empty());
            assert!(state.session.metadata().is_none());
        }

        #[test]
        fn stale_table_detail_loaded_does_not_replace_current_detail() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.session.begin_table_detail_run();
            let current_generation = state.session.selection_generation();
            let _ = state.session.begin_table_detail_run();

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    outcome: Ok(empty_table("public", "users")),
                    generation: current_generation,
                },
                Instant::now(),
            );

            assert!(state.session.table_detail().is_none());
        }

        #[test]
        fn current_table_detail_failure_updates_inspector_and_footer() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let run_id = state.session.begin_table_detail_run();

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    outcome: Err(DbOperationError::PermissionDenied("denied".to_string())),
                    generation,
                },
                Instant::now(),
            );

            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
            assert!(state.messages.last_error().is_some());
        }

        #[test]
        fn metadata_failure_rejects_late_detail_completion() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state
                .session
                .mark_connected(Arc::new(DatabaseMetadata::new("test".to_string())));
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let detail_run_id = state.session.begin_table_detail_run();
            let metadata_run_id = state.session.begin_metadata_refresh();

            route(
                &mut state,
                Action::MetadataLoaded {
                    run_id: metadata_run_id,
                    outcome: Err(DbOperationError::PermissionDenied("denied".to_string())),
                },
                Instant::now(),
            );
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
            assert!(!state.session.is_current_table_detail_run(detail_run_id));

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id: detail_run_id,
                    outcome: Ok(empty_table("public", "users")),
                    generation,
                },
                Instant::now(),
            );

            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
        }

        #[test]
        fn stale_table_detail_failure_does_not_update_current_inspector() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let stale_generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let _ = state.session.begin_table_detail_run();
            let current_generation =
                state
                    .session
                    .select_table("public", "orders", &mut state.query);
            let current_run_id = state.session.begin_table_detail_run();

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id: current_run_id,
                    outcome: Err(DbOperationError::PermissionDenied("denied".to_string())),
                    generation: stale_generation,
                },
                Instant::now(),
            );

            assert_eq!(state.session.selection_generation(), current_generation);
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Loading
            ));
            assert!(state.messages.last_error().is_none());
        }

        #[test]
        fn mysql_reload_metadata_starts_fetch() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "mysql",
                &endpoint::mysql(
                    "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
                    Some("app"),
                ),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            let effects = route(&mut state, Action::ReloadMetadata, Instant::now());

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );
            assert!(state.messages.last_error().is_none());
        }
    }

    mod metadata_loaded {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

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

        fn metadata_loaded_action(state: &mut AppState, metadata: Arc<DatabaseMetadata>) -> Action {
            let run_id = state.session.begin_metadata_refresh();
            Action::MetadataLoaded {
                run_id,
                outcome: Ok(metadata),
            }
        }

        #[test]
        fn table_disappeared_clears_selection_and_cancels_before_effective_user_fetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);

            let metadata = make_metadata(vec![("public", "orders")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = route(&mut state, action, Instant::now());

            assert!(matches!(
                effects.as_slice(),
                [
                    Effect::CancelTrackedTasks,
                    Effect::FetchEffectiveUser { .. }
                ]
            ));

            assert!(state.query.pagination.table().is_empty());
            assert!(state.query.current_result().is_none());
            assert_eq!(state.query.result_generation(), 2);
            assert!(state.session.table_detail().is_none());
            assert!(state.session.selected_table_key().is_none());
            assert_eq!(state.ui.explorer_selected(), 0);
        }

        #[test]
        fn metadata_reload_does_not_cancel_diagnostics_modal_task() {
            let mut state = sqlite_state_with_dsn("sqlite:///tmp/test.db");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            let diagnostics_run_id = state.sqlite_diagnostics.begin_core_fetch();
            state.modal.set_mode(InputMode::SqliteDiagnostics);
            let metadata_run_id = state.session.begin_metadata_refresh();

            let effects = route(
                &mut state,
                Action::MetadataLoaded {
                    run_id: metadata_run_id,
                    outcome: Ok(make_metadata(vec![("public", "orders")])),
                },
                Instant::now(),
            );

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CancelTrackedTasks))
            );
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CancelSqliteDiagnostics))
            );
            assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
            assert!(state.sqlite_diagnostics.is_current_run(diagnostics_run_id));
            assert!(state.sqlite_diagnostics.snapshot().is_none());
        }

        #[test]
        fn table_still_exists_preserves_pagination_and_emits_refresh_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.query.pagination.reset_for_table("public", "users");

            // "orders" comes before "users" alphabetically, so "users" → index 1
            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = route(&mut state, action, Instant::now());

            assert_eq!(state.query.pagination.table(), "users");
            assert_eq!(state.ui.explorer_selected(), 1);
            assert!(matches!(
                effects.as_slice(),
                [Effect::FetchEffectiveUser { .. }, Effect::LoadTable { table, .. }]
                    if table == "users"
            ));
        }

        #[test]
        fn no_table_selected_defaults_to_first() {
            let mut state = state_with_dsn("postgres://localhost/test");

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = route(&mut state, action, Instant::now());

            assert!(matches!(
                effects.as_slice(),
                [Effect::FetchEffectiveUser { .. }]
            ));

            assert_eq!(state.ui.explorer_selected(), 0);
        }
    }

    mod start_er_prefetch_all {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None))
                .collect();
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })
        }

        #[test]
        fn large_db_grows_cache_to_table_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(530)));

            prefetch::start_er_prefetch_all(&mut state);

            assert!(cache_keeps_every_table(&mut state, 530));
        }

        #[test]
        fn sets_fk_expanded_true() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(10)));

            prefetch::start_er_prefetch_all(&mut state);

            assert!(state.er_preparation.fk_expanded());
        }
    }

    mod start_er_prefetch_scoped {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None))
                .collect();
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })
        }

        #[test]
        fn resizes_to_total_table_count_before_processing_scoped_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(560)));
            let tables: Vec<String> = (0..60).map(|i| format!("public.t{i}")).collect();

            let effects = prefetch::start_er_prefetch_scoped(&mut state, tables);

            assert!(matches!(
                effects.as_slice(),
                [Effect::PrefetchTableColumnsAndFks { .. }]
            ));
            assert!(cache_keeps_every_table(&mut state, 560));
        }
    }
}
