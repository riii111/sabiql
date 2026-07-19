use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::{Action, ConnectionTarget};
use crate::update::query_context::termination_effects;

use crate::update::dispatch_result::DispatchResult;

use super::helpers::{reset_for_new_connection, restore_cache, save_current_cache};

pub fn reduce_connection_lifecycle(
    state: &mut AppState,
    action: &Action,
    _now: std::time::Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::TryConnect => {
            if state.session.connection_state().is_not_connected()
                && state.modal.active_mode() == InputMode::Normal
            {
                if let Some(dsn) = state.session.dsn().map(str::to_string) {
                    let run_id = state.session.begin_connecting(&dsn);
                    DispatchResult::handled_with(vec![Effect::FetchMetadata { dsn, run_id }])
                } else {
                    DispatchResult::handled()
                }
            } else {
                DispatchResult::handled()
            }
        }

        Action::SwitchConnection(target) => {
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
            } = target;

            if let Some(current_id) = state.session.active_connection_id().cloned() {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }

            if let Some(cached) = state.connection_caches.get(id).cloned() {
                restore_cache(state, &cached, target);
                let mut effects = vec![Effect::ClearCompletionEngineCache];
                if state.session.effective_user().is_none() {
                    let run_id = state.session.begin_effective_user_fetch();
                    effects.push(Effect::FetchEffectiveUser {
                        dsn: dsn.clone(),
                        run_id,
                    });
                }
                DispatchResult::handled_with(termination_effects(&state.query, effects))
            } else {
                // No cache: reset and fetch metadata
                reset_for_new_connection(state, id, dsn, name, *database_type);
                let run_id = state.session.begin_connecting(dsn);
                DispatchResult::handled_with(termination_effects(
                    &state.query,
                    vec![
                        Effect::ClearCompletionEngineCache,
                        Effect::FetchMetadata {
                            dsn: dsn.clone(),
                            run_id,
                        },
                    ],
                ))
            }
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionId;
    use crate::domain::connection::DatabaseType;
    use crate::model::connection::cache::ConnectionCache;
    use crate::model::connection::state::ConnectionState;
    use crate::model::er_state::ErStatus;
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::model::shared::ui_state::ResultNavMode;
    use crate::test_support::connection::{
        assert_explain_state_cleared, assert_sqlite_diagnostics_cleared,
    };

    fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
        reduce_connection_lifecycle(
            state,
            action,
            std::time::Instant::now(),
            &AppServices::stub(),
        )
        .into_effects()
    }

    fn create_switch_action(id: &ConnectionId, name: &str) -> Action {
        Action::SwitchConnection(ConnectionTarget {
            id: id.clone(),
            dsn: format!("postgres://localhost/{name}"),
            name: name.to_string(),
            database_type: DatabaseType::PostgreSQL,
        })
    }

    mod cache_tests {
        use super::*;

        #[test]
        fn saves_current_cache_before_switching() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            let new_id = ConnectionId::new();

            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
            );
            state.ui.set_explorer_selected_raw(5);
            state.ui.set_inspector_tab(InspectorTab::Indexes);

            let action = create_switch_action(&new_id, "new_db");
            reduce(&mut state, &action);

            let saved = state.connection_caches.get(&current_id).unwrap();
            assert_eq!(saved.explorer_selected, 5);
            assert_eq!(saved.inspector_tab, InspectorTab::Indexes);
        }

        #[test]
        fn restores_cached_state_when_available() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();

            let cached = ConnectionCache {
                explorer_selected: 42,
                inspector_tab: InspectorTab::ForeignKeys,
                ..Default::default()
            };
            state.connection_caches.save(&target_id, cached);

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert_eq!(state.ui.explorer_selected(), 42);
            assert_eq!(state.ui.inspector_tab(), InspectorTab::ForeignKeys);
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
            );
        }

        #[test]
        fn cached_switch_terminates_active_query_run() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());
            let stale_run_id = state.query.begin_running(std::time::Instant::now());

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert!(!state.query.is_running());
            assert!(!state.query.is_current_run(stale_run_id));
        }

        #[test]
        fn preserves_cached_sqlite_ddl_inspector_tab() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            let cached = ConnectionCache {
                explorer_selected: 42,
                inspector_tab: InspectorTab::Ddl,
                ..Default::default()
            };
            state.connection_caches.save(&target_id, cached);

            let action = Action::SwitchConnection(ConnectionTarget {
                id: target_id,
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            reduce(&mut state, &action);

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Ddl);
        }
    }

    mod reconciliation_tests {
        use super::*;
        use crate::domain::SqliteDiagnosticsSnapshot;
        use crate::model::sql_editor::modal::SqlModalTab;

        fn target_action(id: ConnectionId, name: &str, database_type: DatabaseType) -> Action {
            let dsn = match database_type {
                DatabaseType::PostgreSQL => format!("postgres://localhost/{name}"),
                DatabaseType::SQLite => format!("sqlite:///tmp/{name}.db"),
            };
            Action::SwitchConnection(ConnectionTarget {
                id,
                dsn,
                name: name.to_string(),
                database_type,
            })
        }

        fn seed_explain_state(state: &mut AppState) {
            state.explain.set_plan(
                "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
                false,
                0,
                "SELECT * FROM users",
            );
            state.explain.set_plan(
                "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
                false,
                0,
                "SELECT * FROM users WHERE id = 1",
            );
            state.explain.set_error("stale error".to_string());
            assert!(state.explain.plan_text().is_none());
            assert!(state.explain.error().is_some());
            assert!(state.explain.left().is_some());
            assert!(state.explain.right().is_some());
            assert!(!state.explain.history().is_empty());
        }

        fn seed_er_state(state: &mut AppState) {
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .er_preparation
                .queue_pending_table("public.users".to_string());
        }

        fn seed_sqlite_diagnostics(state: &mut AppState) {
            let run_id = state.sqlite_diagnostics.begin_fetch();
            state
                .sqlite_diagnostics
                .set_core_loaded(run_id, SqliteDiagnosticsSnapshot::default());
            assert!(state.sqlite_diagnostics.snapshot().is_some());
            let _ = state.sqlite_diagnostics.begin_quick_check();
            assert!(state.sqlite_diagnostics.is_quick_check_running());
        }

        fn assert_er_state_cleared(state: &AppState) {
            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(state.er_preparation.pending_tables().is_empty());
        }

        fn assert_reconciles_postgres_to_sqlite_feature_state(cached: bool) {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            let target_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
            );
            state.ui.set_inspector_tab(InspectorTab::Rls);
            state.ui.set_inspector_scroll_offset(17);
            state.ui.set_inspector_horizontal_offset(23);
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            seed_er_state(&mut state);
            seed_explain_state(&mut state);

            if cached {
                state.connection_caches.save(
                    &target_id,
                    ConnectionCache {
                        inspector_tab: InspectorTab::Rls,
                        ..Default::default()
                    },
                );
            }

            reduce(
                &mut state,
                &target_action(target_id, "app", DatabaseType::SQLite),
            );

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Info);
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
            assert_eq!(state.ui.inspector_horizontal_offset(), 0);
            assert_eq!(state.sql_modal.active_tab(), SqlModalTab::Sql);
            assert_er_state_cleared(&state);
            assert_explain_state_cleared(&state);
        }

        #[test]
        fn reconciles_postgres_to_sqlite_feature_state_without_cache() {
            assert_reconciles_postgres_to_sqlite_feature_state(false);
        }

        #[test]
        fn reconciles_postgres_to_sqlite_feature_state_with_cache() {
            assert_reconciles_postgres_to_sqlite_feature_state(true);
        }

        fn assert_reconciles_sqlite_diagnostics_on_postgres_switch(cached: bool) {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            let target_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::SQLite,
                "sqlite:///tmp/current.db",
            );
            seed_sqlite_diagnostics(&mut state);
            seed_explain_state(&mut state);

            if cached {
                state
                    .connection_caches
                    .save(&target_id, ConnectionCache::default());
            }

            reduce(
                &mut state,
                &target_action(target_id, "target", DatabaseType::PostgreSQL),
            );

            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
            );
            assert_sqlite_diagnostics_cleared(&state);
            assert_explain_state_cleared(&state);
        }

        #[test]
        fn reconciles_sqlite_diagnostics_when_switching_to_postgres_without_cache() {
            assert_reconciles_sqlite_diagnostics_on_postgres_switch(false);
        }

        #[test]
        fn reconciles_sqlite_diagnostics_when_switching_to_postgres_with_cache() {
            assert_reconciles_sqlite_diagnostics_on_postgres_switch(true);
        }

        #[test]
        fn preserves_postgres_features_when_switching_to_another_postgres_connection() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
            );
            state.ui.set_inspector_tab(InspectorTab::Rls);
            state.sql_modal.set_active_tab(SqlModalTab::Compare);
            seed_er_state(&mut state);
            seed_explain_state(&mut state);

            reduce(
                &mut state,
                &target_action(ConnectionId::new(), "target", DatabaseType::PostgreSQL),
            );

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Rls);
            assert_eq!(state.sql_modal.active_tab(), SqlModalTab::Compare);
            assert_er_state_cleared(&state);
            assert_explain_state_cleared(&state);
        }

        #[test]
        fn preserves_sqlite_features_when_switching_to_another_sqlite_connection() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::SQLite,
                "sqlite:///tmp/current.db",
            );
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.sql_modal.set_active_tab(SqlModalTab::Plan);
            seed_sqlite_diagnostics(&mut state);
            seed_explain_state(&mut state);

            reduce(
                &mut state,
                &target_action(ConnectionId::new(), "target", DatabaseType::SQLite),
            );

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Ddl);
            assert_eq!(state.sql_modal.active_tab(), SqlModalTab::Plan);
            assert_sqlite_diagnostics_cleared(&state);
            assert_explain_state_cleared(&state);
        }
    }

    mod fetching_tests {
        use super::*;

        #[test]
        fn fetches_metadata_when_no_cache_exists() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = create_switch_action(&new_id, "fresh_db");
            let effects = reduce(&mut state, &action).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
        }

        #[test]
        fn sqlite_switch_without_cache_fetches_metadata() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = Action::SwitchConnection(ConnectionTarget {
                id: new_id,
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            let effects = reduce(&mut state, &action).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::SQLite)
            );
        }
    }

    mod cache_restore_tests {
        use super::*;

        #[test]
        fn sqlite_switch_restores_cache() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state.ui.set_explorer_selected_raw(7);
            state.connection_caches.save(
                &target_id,
                ConnectionCache {
                    explorer_selected: 42,
                    inspector_tab: InspectorTab::ForeignKeys,
                    ..Default::default()
                },
            );

            let action = Action::SwitchConnection(ConnectionTarget {
                id: target_id.clone(),
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            let effects = reduce(&mut state, &action).unwrap();

            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );
            assert!(state.connection_caches.get(&target_id).is_some());
            assert_eq!(state.ui.explorer_selected(), 42);
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::SQLite)
            );
            assert_eq!(state.session.connection_state(), ConnectionState::Connected);
        }
    }

    mod pending_state_tests {
        use super::*;

        #[test]
        fn switch_without_cache_clears_pending_er_picker() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .er_preparation
                .queue_pending_table("public.users".to_string());

            let action = create_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(state.er_preparation.pending_tables().is_empty());
        }

        #[test]
        fn cached_switch_clears_pending_er_picker() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .er_preparation
                .queue_pending_table("public.users".to_string());
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(state.er_preparation.pending_tables().is_empty());
        }
    }

    mod connection_state_tests {
        use super::*;

        #[test]
        fn sqlite_try_connect_fetches_metadata() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );
            state
                .session
                .set_connection_state(ConnectionState::NotConnected);

            let effects = reduce(&mut state, &Action::TryConnect).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchMetadata { .. }))
            );
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
        }

        #[test]
        fn updates_active_connection_fields() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = create_switch_action(&new_id, "target_db");
            reduce(&mut state, &action);

            assert_eq!(state.session.active_connection_id(), Some(&new_id));
            assert_eq!(state.session.dsn(), Some("postgres://localhost/target_db"));
            assert_eq!(state.session.active_connection_name(), Some("target_db"));
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
            );
        }

        #[test]
        fn sets_connected_state_when_cache_exists() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();

            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert_eq!(state.session.connection_state(), ConnectionState::Connected);
        }
    }

    mod reset_tests {
        use super::*;

        #[test]
        fn resets_result_selection_when_restoring_cache() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();

            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());
            state.result_interaction.activate_cell(3, 2);

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert_eq!(
                state.result_interaction.selection().mode(),
                ResultNavMode::Scroll
            );
        }

        #[test]
        fn switch_with_cache_resets_sql_prefetch() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());
            let _ = state.sql_modal.begin_prefetch();
            state
                .sql_modal
                .queue_table_prefetch("public.users".to_string());

            let action = create_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(!state.sql_modal.has_pending_prefetch());
        }

        #[test]
        fn switch_without_cache_resets_sql_prefetch() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();
            let _ = state.sql_modal.begin_prefetch();
            state
                .sql_modal
                .queue_table_prefetch("public.users".to_string());

            let action = create_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(!state.sql_modal.has_pending_prefetch());
        }

        #[test]
        fn resets_result_selection_when_no_cache() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            state.result_interaction.activate_cell(5, 0);

            let action = create_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert_eq!(
                state.result_interaction.selection().mode(),
                ResultNavMode::Scroll
            );
        }
    }

    mod switch_effect_tests {
        use super::*;

        #[test]
        fn resets_read_only_on_switch() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();
            state.session.enable_read_only();

            let action = create_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(!state.session.is_read_only());
        }

        #[test]
        fn clears_completion_cache_on_switch() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = create_switch_action(&new_id, "any_db");
            let effects = reduce(&mut state, &action).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
        }
    }
}
