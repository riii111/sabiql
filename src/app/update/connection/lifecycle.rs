use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::connection::state::ConnectionState;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::{Action, ConnectionTarget};
use crate::update::query_context::termination_effects;

use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    mysql_connection_completion_effects, reset_for_database_switch, reset_for_new_connection,
    restore_cache, save_current_cache,
};

pub fn reduce_connection_lifecycle(
    state: &mut AppState,
    action: &Action,
    now: std::time::Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::TryConnect => {
            if state.session.connection_state().is_not_connected()
                && state.modal.active_mode() == InputMode::Normal
            {
                if let Some(dsn) = state.session.dsn().map(str::to_string) {
                    if state.session.active_database_type() == Some(DatabaseType::MySQL) {
                        let target = ConnectionTarget {
                            id: state
                                .session
                                .active_connection_id()
                                .cloned()
                                .expect("active MySQL connection"),
                            dsn,
                            name: state
                                .session
                                .active_connection_name()
                                .unwrap_or_default()
                                .to_string(),
                            database_type: DatabaseType::MySQL,
                            database: state.session.active_database().map(str::to_string),
                        };
                        let run_id = state.session.begin_connection_probe(
                            &target.id,
                            &target.name,
                            target.database_type,
                            &target.dsn,
                            target.database.as_deref(),
                        );
                        state.session.mark_connecting();
                        return DispatchResult::handled_with(vec![Effect::ProbeConnection {
                            target,
                            run_id,
                        }]);
                    }
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
                database,
            } = target;

            if *database_type == DatabaseType::MySQL {
                if state.session.active_database_type() != Some(DatabaseType::MySQL)
                    && let Some(current_id) = state.session.active_connection_id().cloned()
                {
                    let cache = save_current_cache(state);
                    state.connection_caches.save(&current_id, cache);
                }
                let run_id = state.session.begin_connection_probe(
                    id,
                    name,
                    *database_type,
                    dsn,
                    database.as_deref(),
                );
                return DispatchResult::handled_with(vec![Effect::ProbeConnection {
                    target: target.clone(),
                    run_id,
                }]);
            }

            state.session.clear_connection_probe();

            if state.session.active_database_type() != Some(DatabaseType::MySQL)
                && let Some(current_id) = state.session.active_connection_id().cloned()
            {
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
                reset_for_new_connection(state, id, dsn, name, *database_type, database.as_deref());
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

        Action::ConnectionProbeCompleted { target, run_id } => {
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
                database,
            } = target;
            if *database_type != DatabaseType::MySQL
                || !state.session.is_current_connection_probe(
                    id,
                    name,
                    *database_type,
                    dsn,
                    database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            reset_for_new_connection(state, id, dsn, name, *database_type, database.as_deref());
            DispatchResult::handled_with(mysql_connection_completion_effects(
                state,
                dsn,
                database.as_deref(),
            ))
        }

        Action::SwitchMySqlDatabase { database } => {
            if state.session.active_database_type() != Some(DatabaseType::MySQL)
                || !matches!(
                    state.session.connection_state(),
                    ConnectionState::Connected | ConnectionState::AwaitingDatabase
                )
            {
                return DispatchResult::handled();
            }
            let Some(target) = (|| {
                let target = ConnectionTarget {
                    id: state.session.active_connection_id()?.clone(),
                    dsn: state.session.dsn()?.to_string(),
                    name: state.session.active_connection_name()?.to_string(),
                    database_type: DatabaseType::MySQL,
                    database: state.session.active_database().map(str::to_string),
                };
                target.with_database(database)
            })() else {
                state.messages.set_error_at(
                    "Unable to build the selected MySQL database target".to_string(),
                    now,
                );
                return DispatchResult::handled();
            };
            reset_for_database_switch(state, &target);
            state.ui.set_database_picker(false);
            state.modal.set_mode(InputMode::Normal);
            let metadata_run_id = state.session.begin_metadata_refresh();
            DispatchResult::handled_with(termination_effects(
                &state.query,
                vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata {
                        dsn: target.dsn,
                        run_id: metadata_run_id,
                    },
                ],
            ))
        }

        Action::MySqlDatabasesLoaded {
            connection_id,
            dsn,
            connection_generation,
            database_generation,
            databases,
        } => {
            if !state.session.is_current_database_fetch(
                connection_id,
                dsn,
                *connection_generation,
                *database_generation,
            ) {
                return DispatchResult::handled();
            }
            state.session.set_available_databases(databases.clone());
            if state.session.available_databases().is_empty() {
                state
                    .messages
                    .set_error_at("No selectable MySQL databases are visible".to_string(), now);
            }
            DispatchResult::handled()
        }

        Action::MySqlDatabasesFailed {
            connection_id,
            dsn,
            connection_generation,
            database_generation,
            error,
        } => {
            if !state.session.is_current_database_fetch(
                connection_id,
                dsn,
                *connection_generation,
                *database_generation,
            ) {
                return DispatchResult::handled();
            }
            state.messages.set_error_at(error.user_message(), now);
            if state.session.active_database().is_none() {
                state.ui.set_database_picker(false);
                state.session.mark_connection_failed(error.user_message());
                state
                    .connection_error
                    .set_error(ConnectionErrorInfo::from_db_operation_error(error));
                state.modal.replace_mode(InputMode::ConnectionError);
            }
            DispatchResult::handled()
        }

        Action::ConnectionProbeFailed {
            target,
            run_id,
            error,
        } => {
            if target.database_type != DatabaseType::MySQL
                || !state.session.is_current_connection_probe(
                    &target.id,
                    &target.name,
                    target.database_type,
                    &target.dsn,
                    target.database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            if state.session.dsn_matches(&target.dsn) {
                state.session.mark_connection_failed(error.user_message());
            }
            state.connection_error.set_error(
                ConnectionErrorInfo::from_db_operation_error_with_dsn(error, &target.dsn),
            );
            state.modal.replace_mode(InputMode::ConnectionError);
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::connection::DatabaseType;
    use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};
    use crate::domain::{ConnectionId, DatabaseMetadata, MetadataState};
    use crate::model::connection::cache::ConnectionCache;
    use crate::model::connection::error::ConnectionErrorKind;
    use crate::model::connection::state::ConnectionState;
    use crate::model::er_state::ErStatus;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::model::shared::ui_state::ResultNavMode;
    use crate::ports::outbound::DbOperationError;
    use crate::test_support::connection::{
        assert_explain_state_cleared, assert_sqlite_diagnostics_cleared,
    };
    use crate::update::reducer::reduce as reduce_app;

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
            database: None,
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

        fn assert_non_mysql_cache_survives_mysql_switch(
            database_type: DatabaseType,
            dsn: &str,
            name: &str,
        ) {
            let mut state = AppState::new("test".to_string());
            let source_id = ConnectionId::from_string("source");
            let mysql_id = ConnectionId::from_string("mysql");
            let source = ConnectionTarget {
                id: source_id.clone(),
                dsn: dsn.to_string(),
                name: name.to_string(),
                database_type,
                database: None,
            };

            state
                .session
                .activate_connection_with_dsn(&source_id, name, database_type, dsn);
            state.ui.set_explorer_selected_raw(5);
            state.ui.set_inspector_tab(InspectorTab::Indexes);

            let mysql = ConnectionTarget {
                id: mysql_id,
                dsn: "mysql://user@localhost:3306/app".to_string(),
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("app".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switching to MySQL should probe the connection");
            reduce(
                &mut state,
                &Action::ConnectionProbeCompleted {
                    target: mysql,
                    run_id,
                },
            );

            reduce(&mut state, &Action::SwitchConnection(source));

            assert_eq!(state.session.active_connection_id(), Some(&source_id));
            assert_eq!(state.ui.explorer_selected(), 5);
            assert_eq!(state.ui.inspector_tab(), InspectorTab::Indexes);
        }

        #[test]
        fn restores_postgres_cache_after_switching_through_mysql() {
            assert_non_mysql_cache_survives_mysql_switch(
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
                "postgres",
            );
        }

        #[test]
        fn restores_sqlite_cache_after_switching_through_mysql() {
            assert_non_mysql_cache_survives_mysql_switch(
                DatabaseType::SQLite,
                "sqlite:///tmp/current.db",
                "sqlite",
            );
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
                database: None,
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
                DatabaseType::MySQL => format!("mysql://user@localhost/{name}"),
            };
            Action::SwitchConnection(ConnectionTarget {
                id,
                dsn,
                name: name.to_string(),
                database_type,
                database: None,
            })
        }

        fn seed_explain_state(state: &mut AppState) {
            state.explain.set_plan(
                "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
                DatabaseType::PostgreSQL,
                false,
                0,
                "SELECT * FROM users",
            );
            state.explain.set_plan(
                "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
                DatabaseType::PostgreSQL,
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
                database: None,
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
                database: None,
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
        use crate::update::connection::error::reduce_connection_error;

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
        fn mysql_switch_probes_without_fetching_metadata() {
            let mut state = AppState::new("test".to_string());
            let target = ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "mysql://user@localhost:3306/app?ssl-mode=PREFERRED".to_string(),
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("app".to_string()),
            };

            let effects = reduce(&mut state, &Action::SwitchConnection(target.clone())).unwrap();

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::ProbeConnection { target: actual, .. } if actual.dsn == target.dsn
            )));
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );
        }

        #[test]
        fn mysql_probe_completed_fetches_metadata_for_selected_database() {
            let mut state = AppState::new("test".to_string());
            let target = ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "mysql://user@localhost:3306/app".to_string(),
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("app".to_string()),
            };
            let probe_effects = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .expect("switch should start a probe");
            let probe_run_id = probe_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switch should include the probe run");

            let effects = reduce(
                &mut state,
                &Action::ConnectionProbeCompleted {
                    target,
                    run_id: probe_run_id,
                },
            )
            .expect("probe completion should be handled");

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == "mysql://user@localhost:3306/app"
            )));
            assert_eq!(state.session.connection_state(), ConnectionState::Connected);
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);
        }

        #[test]
        fn mysql_switch_invalidates_previous_metadata_failure() {
            let mut state = AppState::new("test".to_string());
            let postgres = ConnectionTarget {
                id: ConnectionId::from_string("postgres-b"),
                dsn: "postgres://localhost/b".to_string(),
                name: "postgres-b".to_string(),
                database_type: DatabaseType::PostgreSQL,
                database: None,
            };
            let postgres_effects =
                reduce(&mut state, &Action::SwitchConnection(postgres.clone())).unwrap();
            let postgres_run_id = postgres_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::FetchMetadata { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            let mysql = ConnectionTarget {
                id: ConnectionId::from_string("mysql-c"),
                dsn: "mysql://user@localhost:3306/c?ssl-mode=PREFERRED".to_string(),
                name: "mysql-c".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("c".to_string()),
            };
            let mysql_effects =
                reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let mysql_run_id = mysql_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MetadataFailed {
                    dsn: postgres.dsn,
                    run_id: postgres_run_id,
                    error: DbOperationError::ConnectionFailed("stale postgres".to_string()),
                },
            );
            reduce(
                &mut state,
                &Action::ConnectionProbeCompleted {
                    target: mysql,
                    run_id: mysql_run_id,
                },
            );

            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::MySQL)
            );
            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.modal.active_mode(), InputMode::Normal);
            assert!(state.connection_error.error_info().is_none());
        }

        #[test]
        fn mysql_probe_failure_does_not_leave_previous_connecting_state() {
            let mut state = AppState::new("test".to_string());
            let postgres = ConnectionTarget {
                id: ConnectionId::from_string("postgres-b"),
                dsn: "postgres://localhost/b".to_string(),
                name: "postgres-b".to_string(),
                database_type: DatabaseType::PostgreSQL,
                database: None,
            };
            let _ = reduce(&mut state, &Action::SwitchConnection(postgres)).unwrap();

            let mysql = ConnectionTarget {
                id: ConnectionId::from_string("mysql-c"),
                dsn: "mysql://user@localhost:3306/c?ssl-mode=PREFERRED".to_string(),
                name: "mysql-c".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("c".to_string()),
            };
            let mysql_effects =
                reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let mysql_run_id = mysql_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: mysql,
                    run_id: mysql_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            assert!(state.session.connection_state().is_not_connected());
            assert!(matches!(
                state.session.metadata_state(),
                MetadataState::NotLoaded
            ));
            assert!(!state.session.is_reloading());
        }

        #[test]
        fn mysql_probe_failure_finishes_previous_reload() {
            let mut state = AppState::new("test".to_string());
            let postgres_id = ConnectionId::from_string("postgres-b");
            state.session.activate_connection_with_dsn(
                &postgres_id,
                "postgres-b",
                DatabaseType::PostgreSQL,
                "postgres://localhost/b",
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let _ = state.session.begin_reload();
            assert!(state.session.is_reloading());

            let mysql = ConnectionTarget {
                id: ConnectionId::from_string("mysql-c"),
                dsn: "mysql://user@localhost:3306/c?ssl-mode=PREFERRED".to_string(),
                name: "mysql-c".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("c".to_string()),
            };
            let mysql_effects =
                reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let mysql_run_id = mysql_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: mysql,
                    run_id: mysql_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            assert!(state.session.connection_state().is_connected());
            assert!(!state.session.is_reloading());
        }

        #[test]
        fn stale_mysql_probe_completion_is_ignored_after_switch() {
            let mut state = AppState::new("test".to_string());
            let first = ConnectionTarget {
                id: ConnectionId::from_string("mysql-a"),
                dsn: "mysql://user@localhost:3306/a?ssl-mode=PREFERRED".to_string(),
                name: "mysql-a".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("a".to_string()),
            };
            let second = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };

            let first_effects =
                reduce(&mut state, &Action::SwitchConnection(first.clone())).unwrap();
            let first_run_id = first_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(&mut state, &Action::SwitchConnection(second));

            reduce(
                &mut state,
                &Action::ConnectionProbeCompleted {
                    target: first,
                    run_id: first_run_id,
                },
            );

            assert_eq!(state.session.active_connection_id(), None);
            assert_eq!(state.session.dsn(), None);
        }

        #[test]
        fn stale_mysql_probe_failure_is_ignored_after_switch() {
            let mut state = AppState::new("test".to_string());
            let first = ConnectionTarget {
                id: ConnectionId::from_string("mysql-a"),
                dsn: "mysql://user@localhost:3306/a?ssl-mode=PREFERRED".to_string(),
                name: "mysql-a".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("a".to_string()),
            };
            let second = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };

            let first_effects =
                reduce(&mut state, &Action::SwitchConnection(first.clone())).unwrap();
            let first_run_id = first_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(&mut state, &Action::SwitchConnection(second));

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: first,
                    run_id: first_run_id,
                    error: DbOperationError::ConnectionFailed("stale".to_string()),
                },
            );

            assert_eq!(state.modal.active_mode(), InputMode::Normal);
            assert!(state.connection_error.error_info().is_none());
        }

        #[test]
        fn retry_after_mysql_switch_failure_reprobes_failed_target() {
            let mut state = AppState::new("test".to_string());
            let target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(target.clone())).unwrap();
            let first_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: target.clone(),
                    run_id: first_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );
            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();

            let (retry_target, retry_run_id) = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { target, run_id } => Some((target, *run_id)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(retry_target.dsn, target.dsn);
            assert_eq!(retry_target.id, target.id);
            assert_ne!(retry_run_id, first_run_id);
        }

        #[test]
        fn retry_after_mysql_switch_failure_preserves_connected_previous_target() {
            let mut state = AppState::new("test".to_string());
            let postgres_id = ConnectionId::from_string("postgres-a");
            let postgres_dsn = "postgres://localhost/a".to_string();
            state.session.activate_connection_with_dsn(
                &postgres_id,
                "postgres-a",
                DatabaseType::PostgreSQL,
                &postgres_dsn,
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            state
                .session
                .set_metadata(Some(Arc::new(DatabaseMetadata::new("a".to_string()))));
            state.session.set_metadata_state(MetadataState::Loaded);

            let mysql = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let first_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: mysql.clone(),
                    run_id: first_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();
            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.session.metadata_state(), &MetadataState::Loaded);
            let retry_run_id = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: mysql,
                    run_id: retry_run_id,
                    error: DbOperationError::ConnectionFailed("refused again".to_string()),
                },
            );

            assert_eq!(state.session.dsn(), Some(postgres_dsn.as_str()));
            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.session.metadata_state(), &MetadataState::Loaded);
            assert!(!state.session.is_reloading());
        }

        #[test]
        fn postgres_reload_retry_does_not_use_old_mysql_probe_target() {
            let mut state = AppState::new("test".to_string());
            let postgres_id = ConnectionId::from_string("postgres-a");
            let postgres_dsn = "postgres://localhost/a".to_string();
            state.session.activate_connection_with_dsn(
                &postgres_id,
                "postgres-a",
                DatabaseType::PostgreSQL,
                &postgres_dsn,
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            let mysql = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(mysql.clone())).unwrap();
            let mysql_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target: mysql,
                    run_id: mysql_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            let reload_run_id = state.session.begin_reload();
            assert!(state.session.pending_connection_probe().is_none());
            reduce_app(
                &mut state,
                Action::MetadataFailed {
                    dsn: postgres_dsn.clone(),
                    run_id: reload_run_id,
                    error: DbOperationError::ConnectionFailed("reload refused".to_string()),
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();
            assert!(retry_effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == &postgres_dsn
            )));
            assert!(
                !retry_effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::ProbeConnection { .. }))
            );
        }

        #[test]
        fn mysql_probe_without_database_enters_awaiting_database() {
            let mut state = AppState::new("test".to_string());
            let target = ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "mysql://user@localhost:3306?ssl-mode=PREFERRED".to_string(),
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: None,
            };

            let run_id = state.session.begin_connection_probe(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                target.database.as_deref(),
            );
            let effects = reduce(
                &mut state,
                &Action::ConnectionProbeCompleted { target, run_id },
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::ClearCompletionEngineCache))
            );
            assert!(state.session.connection_state().is_awaiting_database());
            assert!(matches!(
                state.session.metadata_state(),
                MetadataState::NotLoaded
            ));
        }

        #[test]
        fn mysql_probe_failure_never_enters_connected_state() {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::new();
            let dsn = "mysql://user@localhost:3306/app?ssl-mode=PREFERRED".to_string();
            state.session.activate_connection_with_target(
                &id,
                "mysql",
                DatabaseType::MySQL,
                &dsn,
                Some("app"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connecting);
            let target = ConnectionTarget {
                id,
                dsn,
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("app".to_string()),
            };
            let run_id = state.session.begin_connection_probe(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                target.database.as_deref(),
            );

            reduce(
                &mut state,
                &Action::ConnectionProbeFailed {
                    target,
                    run_id,
                    error: DbOperationError::ConnectionFailed(
                        "ERROR 1045: Access denied for user 'user'".to_string(),
                    ),
                },
            );

            assert!(!state.session.connection_state().is_connected());
            assert!(state.session.connection_state().is_failed());
            assert_eq!(state.modal.active_mode(), InputMode::ConnectionError);
            assert_eq!(
                state.connection_error.error_info().unwrap().kind,
                ConnectionErrorKind::AuthFailed
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

    mod mysql_database_tests {
        use super::*;
        use crate::update::action::ErDiagramInfo;

        fn mysql_target(id: &ConnectionId, database: Option<&str>) -> ConnectionTarget {
            let database = database.map(str::to_string);
            let dsn = format!(
                "mysql://user:secret@localhost:3306/{}?ssl-mode=PREFERRED",
                database.as_deref().unwrap_or("")
            );
            ConnectionTarget {
                id: id.clone(),
                dsn,
                name: "mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database,
            }
        }

        #[test]
        fn connection_without_database_opens_picker_and_fetches_server_databases() {
            let mut state = AppState::new("test".to_string());
            state.ui.table_picker_mut().set_pane_height(5);
            state.ui.table_picker_mut().insert_filter_str("old table");
            state.ui.table_picker_mut().set_selection(8);
            let id = ConnectionId::from_string("mysql");
            let target = mysql_target(&id, None);
            let probe_run_id = state.session.begin_connection_probe(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                None,
            );

            let effects = reduce(
                &mut state,
                &Action::ConnectionProbeCompleted {
                    target,
                    run_id: probe_run_id,
                },
            )
            .unwrap();

            assert!(state.session.connection_state().is_awaiting_database());
            assert_eq!(state.session.active_database(), None);
            assert!(state.ui.database_picker());
            assert_eq!(state.input_mode(), InputMode::TablePicker);
            assert_eq!(state.ui.table_picker().filter_input().content(), "");
            assert_eq!(state.ui.table_picker().selected(), 0);
            assert_eq!(state.ui.table_picker().scroll_offset(), 0);
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMySqlDatabases { connection_id, .. } if connection_id == &id
            )));
        }

        #[test]
        fn database_selection_resets_context_and_preserves_read_only() {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::from_string("mysql");
            let target = mysql_target(&id, Some("app"));
            state.session.activate_connection_with_target(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                target.database.as_deref(),
            );
            state.session.mark_probe_connected(true);
            state.session.enable_read_only();
            state.session.set_available_databases(vec![
                "information_schema".to_string(),
                "analytics".to_string(),
            ]);
            state.ui.set_database_picker(true);
            state.modal.set_mode(InputMode::TablePicker);
            let stale_er_run_id = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_rendering();
            state
                .query_history_picker
                .replace_entries(&[QueryHistoryEntry::new(
                    "SELECT old_database".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    id,
                    QueryResultStatus::Success,
                    None,
                )]);

            let effects = reduce_app(
                &mut state,
                Action::ConfirmSelection,
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.session.active_database(), Some("analytics"));
            assert!(
                state
                    .session
                    .dsn()
                    .is_some_and(|dsn| dsn.contains("/analytics"))
            );
            assert!(state.session.metadata().is_none());
            assert!(state.session.is_read_only());
            assert_eq!(
                state.session.available_databases(),
                ["analytics".to_string()]
            );
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.query_history_picker.entries().is_empty());
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );

            let effects = reduce_app(
                &mut state,
                Action::ErDiagramOpened(ErDiagramInfo {
                    run_id: stale_er_run_id,
                    path: "stale.svg".to_string(),
                    table_count: 1,
                    total_tables: 1,
                }),
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.messages.last_success().is_none());
        }

        #[test]
        fn database_switch_closes_query_history_picker_and_discards_entries() {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::from_string("mysql");
            let target = mysql_target(&id, Some("app"));
            state.session.activate_connection_with_target(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                target.database.as_deref(),
            );
            state.session.mark_probe_connected(true);
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            state.sql_modal.completion_mut_for_test().visible = true;
            state.explain.set_plan(
                "-> Table scan on users  (cost=1 rows=10)".to_string(),
                DatabaseType::MySQL,
                false,
                1,
                "SELECT * FROM users",
            );
            state.explain.set_plan(
                "-> Index lookup on users  (cost=0.5 rows=1)".to_string(),
                DatabaseType::MySQL,
                false,
                1,
                "SELECT * FROM users WHERE id = 1",
            );
            state
                .query_history_picker
                .replace_entries(&[QueryHistoryEntry::new_with_database(
                    "SELECT from app".to_string(),
                    "2026-03-13T12:00:00Z".to_string(),
                    id,
                    Some("app".to_string()),
                    QueryResultStatus::Success,
                    None,
                )]);

            let effects = reduce_app(
                &mut state,
                Action::SwitchMySqlDatabase {
                    database: "analytics".to_string(),
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.session.active_database(), Some("analytics"));
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.query_history_picker.entries().is_empty());
            assert!(!state.sql_modal.completion().visible);
            assert!(state.explain.left().is_none());
            assert!(state.explain.right().is_none());
            assert!(state.explain.history().is_empty());
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::ClearCompletionEngineCache))
            );
        }

        #[test]
        fn stale_database_list_from_previous_connection_generation_is_ignored() {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::from_string("mysql");
            let target = mysql_target(&id, None);
            state.session.activate_connection_with_target(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                None,
            );
            state.session.mark_probe_connected(false);
            let stale_generation = state.session.connection_generation();
            let _ = state.session.begin_connection_probe(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                None,
            );
            let server_dsn = state.session.server_dsn().unwrap();
            let database_generation = state.session.database_generation();

            let _ = reduce(
                &mut state,
                &Action::MySqlDatabasesLoaded {
                    connection_id: id,
                    dsn: server_dsn,
                    connection_generation: stale_generation,
                    database_generation,
                    databases: vec!["stale".to_string()],
                },
            );

            assert!(state.session.available_databases().is_empty());
        }
    }
}
