use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::{Action, ConnectionTarget};
use crate::update::helpers::metadata_reload_effects;
use crate::update::query_context::termination_effects;

use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    mysql_connection_completion_effects, reset_for_new_connection, restore_cache,
    save_current_connection_cache,
};

fn clear_query_confirmation(state: &mut AppState) {
    state.confirm_dialog.take_intent();
    state.sql_modal.enter_normal();
}

pub fn reduce_connection_lifecycle(
    state: &mut AppState,
    action: &Action,
    _now: std::time::Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::TryConnect => DispatchResult::handled_with(try_connect(state)),

        Action::SwitchConnection(target) => {
            state.session.cancel_connection_save_and_disconnect();
            state.connection_error.clear();
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
                database,
            } = target;

            if *database_type == DatabaseType::MySQL && database.is_none() {
                state
                    .messages
                    .set_error("MySQL connection field `database` is required".to_string());
                return DispatchResult::handled();
            }

            save_current_connection_cache(state);
            clear_query_confirmation(state);

            if *database_type == DatabaseType::MySQL {
                let run_id =
                    state
                        .session
                        .begin_mysql_connection_probe(id, name, dsn, database.as_deref());
                state.query.reset_for_context_change();
                return DispatchResult::handled_with(termination_effects(
                    &state.query,
                    vec![Effect::ProbeMySqlConnection {
                        target: target.clone(),
                        run_id,
                    }],
                ));
            }

            state.session.clear_mysql_connection_probe();

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
                reset_for_new_connection(state, target);
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

        Action::MySqlConnectionProbeCompleted {
            target,
            run_id,
            lower_case_table_names,
        } => {
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
                database,
            } = target;
            if *database_type != DatabaseType::MySQL
                || !state.session.is_current_mysql_connection_probe(
                    id,
                    name,
                    dsn,
                    database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            let cached = state.connection_caches.get(id).cloned();
            if let Some(cached) =
                cached.filter(|cache| cache.is_valid_mysql_snapshot(dsn, database.as_deref()))
            {
                restore_cache(state, &cached, target);
                state
                    .session
                    .set_mysql_lower_case_table_names(*lower_case_table_names);
                let reload_effects = metadata_reload_effects(state, dsn);
                return DispatchResult::handled_with(termination_effects(
                    &state.query,
                    reload_effects,
                ));
            }

            state.connection_caches.remove(id);
            reset_for_new_connection(state, target);
            state
                .session
                .set_mysql_lower_case_table_names(*lower_case_table_names);
            DispatchResult::handled_with(mysql_connection_completion_effects(state, dsn))
        }

        Action::MySqlConnectionProbeFailed {
            target,
            run_id,
            error,
        } => {
            if target.database_type != DatabaseType::MySQL
                || !state.session.is_current_mysql_connection_probe(
                    &target.id,
                    &target.name,
                    &target.dsn,
                    target.database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            let message = error.user_message();
            let table_detail_retry = if state.session.dsn_matches(&target.dsn) {
                None
            } else {
                state.session.retry_table_detail_after_probe_failure().map(
                    |(dsn, generation, run_id)| Effect::FetchTableDetail {
                        dsn,
                        schema: state.query.pagination.schema().to_string(),
                        table: state.query.pagination.table().to_string(),
                        generation,
                        run_id,
                    },
                )
            };
            if state.session.dsn_matches(&target.dsn) {
                state
                    .session
                    .mark_table_detail_probe_failed(&target.dsn, message);
                state.session.mark_connection_failed();
            }
            state
                .connection_error
                .set_connection_switch_error(ConnectionErrorInfo::from_db_operation_error(error));
            state.modal.replace_mode(InputMode::ConnectionError);
            DispatchResult::handled_with(table_detail_retry.into_iter().collect())
        }

        _ => DispatchResult::pass(),
    }
}

pub(super) fn try_connect(state: &mut AppState) -> Vec<Effect> {
    state.session.cancel_connection_save_and_disconnect();
    if state.session.connection_state().is_not_connected()
        && state.modal.active_mode() == InputMode::Normal
    {
        if let Some(dsn) = state.session.dsn().map(str::to_string) {
            if state.session.active_database_type() == Some(DatabaseType::MySQL) {
                if state.session.active_database().is_none() {
                    state
                        .messages
                        .set_error("MySQL connection field `database` is required".to_string());
                    return vec![];
                }
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
                let run_id = state.session.begin_mysql_connection_probe(
                    &target.id,
                    &target.name,
                    &target.dsn,
                    target.database.as_deref(),
                );
                clear_query_confirmation(state);
                state.query.reset_for_context_change();
                state.session.mark_connecting();
                return vec![Effect::ProbeMySqlConnection { target, run_id }];
            }
            let run_id = state.session.begin_connecting(&dsn);
            vec![Effect::FetchMetadata { dsn, run_id }]
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::connection::DatabaseType;
    use crate::domain::{
        ConnectionId, DatabaseMetadata, MetadataState, QueryResult, QuerySource, Table,
        TableKindInfo, TableStorageAttributes, TableSummary,
    };
    use crate::model::browse::query_execution::PaginationState;
    use crate::model::browse::session::TableDetailState;
    use crate::model::connection::cache::ConnectionCache;
    use crate::model::connection::error::test_support;
    use crate::model::connection::state::ConnectionState;
    use crate::model::er_state::ErStatus;
    use crate::model::shared::confirm_dialog::ConfirmIntent;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::model::shared::ui_state::ResultNavMode;
    use crate::model::sql_editor::modal::SqlModalStatus;
    use crate::ports::outbound::{ConnectionFailureKind, DbOperationError};
    use crate::test_support::connection::{
        assert_explain_state_cleared, assert_sqlite_diagnostics_cleared,
    };
    use crate::update::connection::error::reduce_connection_error;
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

    fn create_postgres_switch_action(id: &ConnectionId, name: &str) -> Action {
        Action::SwitchConnection(ConnectionTarget {
            id: id.clone(),
            dsn: format!("postgres://localhost/{name}"),
            name: name.to_string(),
            database_type: DatabaseType::PostgreSQL,
            database: None,
        })
    }

    mod metadata_reload_race {
        use super::*;

        fn mysql_target(id: &str, database: &str) -> ConnectionTarget {
            ConnectionTarget {
                id: ConnectionId::from_string(id),
                dsn: format!("mysql://user@localhost:3306/{database}"),
                name: id.to_string(),
                database_type: DatabaseType::MySQL,
                database: Some(database.to_string()),
            }
        }

        fn active_mysql_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            let current = mysql_target("mysql-a", "a");
            state.session.activate_connection_with_target(
                &current.id,
                &current.name,
                current.database_type,
                &current.dsn,
                current.database.as_deref(),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            state
        }

        #[test]
        fn reload_during_switch_preserves_probe_until_success() {
            let mut state = active_mysql_state();
            let target = mysql_target("mysql-b", "b");
            let probe_effects = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .expect("switch should start a probe");
            let probe_run_id = probe_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switch should include the probe run");

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(reload_effects.is_empty());
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(probe_run_id)
            );
            assert_eq!(
                state.messages.last_error(),
                Some("Connection switch in progress")
            );

            let completion_effects = reduce_app(
                &mut state,
                Action::MySqlConnectionProbeCompleted {
                    target: target.clone(),
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.session.active_connection_id(), Some(&target.id));
            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(completion_effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == &target.dsn
            )));

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );
            assert!(reload_effects.iter().any(|effect| matches!(
                effect,
                Effect::Sequence(effects)
                    if effects.iter().any(|effect| matches!(
                        effect,
                        Effect::FetchMetadata { dsn, .. } if dsn == &target.dsn
                    ))
            )));
        }

        #[test]
        fn same_mysql_retry_blocks_reload_until_probe_success() {
            let mut state = active_mysql_state();
            let target = mysql_target("mysql-a", "a");
            state.session.set_connection_state(ConnectionState::Failed);
            state.connection_error.set_error(test_support::from_parts(
                "Connection refused",
                "Check the host, port, and server availability",
                true,
                "connection refused",
            ));

            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();
            let retry_run_id = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("retry should start a MySQL probe");

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(reload_effects.is_empty());
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(retry_run_id)
            );

            let completion_effects = reduce_app(
                &mut state,
                Action::MySqlConnectionProbeCompleted {
                    target: target.clone(),
                    run_id: retry_run_id,
                    lower_case_table_names: 0,
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(completion_effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == &target.dsn
            )));

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );
            assert!(reload_effects.iter().any(|effect| matches!(
                effect,
                Effect::Sequence(effects)
                    if effects.iter().any(|effect| matches!(
                        effect,
                        Effect::FetchMetadata { dsn, .. } if dsn == &target.dsn
                    ))
            )));
        }

        #[test]
        fn reload_during_switch_preserves_probe_until_failure() {
            let mut state = active_mysql_state();
            let target = mysql_target("mysql-b", "b");
            let probe_effects = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .expect("switch should start a probe");
            let probe_run_id = probe_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switch should include the probe run");

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(reload_effects.is_empty());
            let failure_effects = reduce_app(
                &mut state,
                Action::MySqlConnectionProbeFailed {
                    target: target.clone(),
                    run_id: probe_run_id,
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(failure_effects.is_empty());
            assert_eq!(
                state.session.active_connection_id(),
                Some(&ConnectionId::from_string("mysql-a"))
            );
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(probe_run_id)
            );
            assert_eq!(state.modal.active_mode(), InputMode::ConnectionError);
            assert!(state.connection_error.can_retry());

            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();
            assert!(retry_effects.iter().any(|effect| matches!(
                effect,
                Effect::ProbeMySqlConnection { target: retry_target, .. }
                    if retry_target.id == target.id
                        && retry_target.dsn == target.dsn
                        && retry_target.database == target.database
            )));
        }

        #[test]
        fn pending_probe_blocks_adhoc_on_old_connection_and_discards_confirmation() {
            let mut state = active_mysql_state();
            state.sql_modal.enter_editing();
            state.confirm_dialog.open(
                "Confirm UPDATE",
                "",
                ConfirmIntent::ExecuteWrite {
                    sql: "UPDATE accounts SET name = 'wrong'".to_string(),
                    blocked: false,
                },
            );
            let target = mysql_target("mysql-b", "b");

            reduce(&mut state, &Action::SwitchConnection(target));

            assert_eq!(state.session.dsn(), Some("mysql://user@localhost:3306/a"));
            assert!(state.session.pending_mysql_connection_probe().is_some());
            assert!(matches!(state.sql_modal.status(), SqlModalStatus::Normal));
            assert!(state.confirm_dialog.intent().is_none());

            let effects = reduce_app(
                &mut state,
                Action::ExecuteAdhoc("DROP TABLE accounts".to_string()),
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(!state.query.is_running());
        }
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

            let action = create_postgres_switch_action(&new_id, "new_db");
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switching to MySQL should probe the connection");
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: mysql,
                    run_id,
                    lower_case_table_names: 0,
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
        fn saves_current_mysql_cache_before_switching() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::from_string("mysql-current");
            state.session.activate_connection_with_target(
                &current_id,
                "current",
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/current",
                Some("current"),
            );
            state
                .session
                .mark_connected(Arc::new(DatabaseMetadata::new("current".to_string())));
            state
                .session
                .mark_effective_user_loaded(Some("user@localhost".to_string()));
            state.ui.set_explorer_selected_raw(5);
            state.ui.set_inspector_tab(InspectorTab::Indexes);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT 1".to_string(),
                    vec!["value".to_string()],
                    vec![vec!["1".to_string()]],
                    1,
                    QuerySource::Preview,
                )));

            reduce(
                &mut state,
                &create_postgres_switch_action(&ConnectionId::from_string("postgres"), "postgres"),
            );

            let cache = state.connection_caches.get(&current_id).unwrap();
            assert!(
                cache.is_valid_mysql_snapshot(
                    "mysql://user@localhost:3306/current",
                    Some("current")
                )
            );
            assert_eq!(cache.explorer_selected, 5);
            assert_eq!(cache.inspector_tab, InspectorTab::Indexes);
            assert!(cache.query_result.is_some());
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

            let action = create_postgres_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert_eq!(state.ui.explorer_selected(), 42);
            assert_eq!(state.ui.inspector_tab(), InspectorTab::ForeignKeys);
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
            );
            assert_eq!(state.session.connection_state(), ConnectionState::Connected);
        }

        #[test]
        fn cached_switch_terminates_active_query_run() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());
            let stale_run_id = state.query.begin_running(std::time::Instant::now());

            let action = create_postgres_switch_action(&target_id, "cached_db");
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

        #[derive(Clone, Copy)]
        enum NonMySqlDatabaseType {
            PostgreSQL,
            SQLite,
        }

        fn target_action(
            id: ConnectionId,
            name: &str,
            database_type: NonMySqlDatabaseType,
        ) -> Action {
            let (dsn, database_type) = match database_type {
                NonMySqlDatabaseType::PostgreSQL => (
                    format!("postgres://localhost/{name}"),
                    DatabaseType::PostgreSQL,
                ),
                NonMySqlDatabaseType::SQLite => {
                    (format!("sqlite:///tmp/{name}.db"), DatabaseType::SQLite)
                }
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
        }

        fn seed_er_state(state: &mut AppState) {
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
        }

        fn seed_sqlite_diagnostics(state: &mut AppState) {
            let run_id = state.sqlite_diagnostics.begin_core_fetch();
            state
                .sqlite_diagnostics
                .set_core_loaded(run_id, SqliteDiagnosticsSnapshot::default());
            let _ = state.sqlite_diagnostics.begin_quick_check();
        }

        fn assert_er_state_cleared(state: &AppState) {
            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(!state.table_prefetch.has_pending_prefetch());
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
                &target_action(target_id, "app", NonMySqlDatabaseType::SQLite),
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
                &target_action(target_id, "target", NonMySqlDatabaseType::PostgreSQL),
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
                &target_action(
                    ConnectionId::new(),
                    "target",
                    NonMySqlDatabaseType::PostgreSQL,
                ),
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
                &target_action(ConnectionId::new(), "target", NonMySqlDatabaseType::SQLite),
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

            let action = create_postgres_switch_action(&new_id, "fresh_db");
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
            assert_eq!(state.session.active_connection_id(), Some(&new_id));
            assert_eq!(state.session.dsn(), Some("postgres://localhost/fresh_db"));
            assert_eq!(state.session.active_connection_name(), Some("fresh_db"));
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
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

        fn valid_mysql_cache(dsn: &str, database: &str) -> ConnectionCache {
            let mut pagination = PaginationState::default();
            pagination.reset_for_table("app", "users");
            pagination.set_page_result(2, false);
            ConnectionCache {
                connection_dsn: Some(dsn.to_string()),
                database_type: Some(DatabaseType::MySQL),
                database: Some(database.to_string()),
                metadata: Some(Arc::new(DatabaseMetadata::new(database.to_string()))),
                effective_user: Some("user@localhost".to_string()),
                selected_table_key: Some("app.users".to_string()),
                query_result: Some(Arc::new(QueryResult::success(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string()],
                    vec![vec!["1".to_string()]],
                    1,
                    QuerySource::Preview,
                ))),
                pagination,
                explorer_selected: 42,
                inspector_tab: InspectorTab::ForeignKeys,
                ..Default::default()
            }
        }

        fn mysql_target(id: &ConnectionId, dsn: &str, database: &str) -> ConnectionTarget {
            ConnectionTarget {
                id: id.clone(),
                dsn: dsn.to_string(),
                name: database.to_string(),
                database_type: DatabaseType::MySQL,
                database: Some(database.to_string()),
            }
        }

        #[test]
        fn mysql_switch_restores_valid_cache_and_revalidates_metadata() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::from_string("mysql-target");
            let target = mysql_target(&target_id, "mysql://user@localhost:3306/app", "app");
            state.connection_caches.save(
                &target_id,
                valid_mysql_cache(&target.dsn, target.database.as_deref().unwrap()),
            );

            let probe_effects = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .expect("switch should start a probe");
            let probe_run_id = probe_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switch should include the probe run");

            let effects = reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target,
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
            )
            .expect("probe completion should be handled");

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CancelTrackedTasks))
            );
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchEffectiveUser { .. }))
            );
            let metadata_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Sequence(effects) => effects.iter().find_map(|effect| match effect {
                        Effect::FetchMetadata { run_id, .. } => Some(*run_id),
                        _ => None,
                    }),
                    _ => None,
                })
                .expect("cached MySQL state should start metadata revalidation");
            assert!(state.session.is_reloading());
            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.session.database_name(), Some("app"));
            assert_eq!(state.session.effective_user(), Some("user@localhost"));
            assert_eq!(state.session.selected_table_key(), Some("app.users"));
            assert!(state.query.current_result().is_some());
            assert_eq!(state.query.pagination.schema(), "app");
            assert_eq!(state.query.pagination.table(), "users");
            assert_eq!(state.query.pagination.current_page(), 2);
            assert_eq!(state.ui.explorer_selected(), 42);
            assert_eq!(state.ui.inspector_tab(), InspectorTab::ForeignKeys);

            let refreshed_metadata = Arc::new({
                let mut metadata = DatabaseMetadata::new("app".to_string());
                metadata.table_summaries.push(TableSummary::new(
                    "app".to_string(),
                    "users".to_string(),
                    Some(1),
                    false,
                ));
                metadata.table_summaries.push(TableSummary::new(
                    "app".to_string(),
                    "orders".to_string(),
                    Some(10),
                    false,
                ));
                metadata
            });
            let refresh_effects = reduce_app(
                &mut state,
                Action::MetadataLoaded {
                    dsn: "mysql://user@localhost:3306/app".to_string(),
                    run_id: metadata_run_id,
                    metadata: refreshed_metadata,
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.tables().iter().any(|table| table.name == "orders"));
            assert!(refresh_effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchTableDetail { schema, table, .. }
                    if schema == "app" && table == "users"
            )));
        }

        #[test]
        fn mysql_switch_ignores_cache_for_stale_dsn_or_database() {
            for (cached_dsn, cached_database) in [
                ("mysql://user@localhost:3306/old", "app"),
                ("mysql://user@localhost:3306/app", "old"),
            ] {
                let mut state = AppState::new("test".to_string());
                let target_id = ConnectionId::from_string("mysql-target");
                let target = mysql_target(&target_id, "mysql://user@localhost:3306/app", "app");
                state
                    .connection_caches
                    .save(&target_id, valid_mysql_cache(cached_dsn, cached_database));

                let probe_effects =
                    reduce(&mut state, &Action::SwitchConnection(target.clone())).unwrap();
                let probe_run_id = probe_effects
                    .iter()
                    .find_map(|effect| match effect {
                        Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                        _ => None,
                    })
                    .unwrap();
                let effects = reduce(
                    &mut state,
                    &Action::MySqlConnectionProbeCompleted {
                        target,
                        run_id: probe_run_id,
                        lower_case_table_names: 0,
                    },
                )
                .unwrap();

                assert!(
                    effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
                );
                assert!(state.session.connection_state().is_connecting());
                assert!(state.connection_caches.get(&target_id).is_none());
            }
        }

        #[test]
        fn stale_cached_metadata_completion_is_ignored_after_switching_back() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::from_string("mysql-target");
            let target = mysql_target(&target_id, "mysql://user@localhost:3306/app", "app");
            state.connection_caches.save(
                &target_id,
                valid_mysql_cache(&target.dsn, target.database.as_deref().unwrap()),
            );

            let probe_run_id = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .unwrap()
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: target.clone(),
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
            );
            let stale_run_id = state.session.metadata_generation();

            let other_id = ConnectionId::from_string("mysql-other");
            let other = mysql_target(&other_id, "mysql://user@localhost:3306/other", "other");
            let other_probe_run_id = reduce(&mut state, &Action::SwitchConnection(other.clone()))
                .unwrap()
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: other,
                    run_id: other_probe_run_id,
                    lower_case_table_names: 0,
                },
            );

            let probe_run_id = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .unwrap()
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: target.clone(),
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
            );

            reduce_app(
                &mut state,
                Action::MetadataLoaded {
                    dsn: target.dsn,
                    run_id: stale_run_id,
                    metadata: Arc::new(DatabaseMetadata::new("stale".to_string())),
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.session.database_name(), Some("app"));
        }

        #[test]
        fn mysql_probe_failure_does_not_restore_cached_target() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::from_string("mysql-target");
            let target = mysql_target(&target_id, "mysql://user@localhost:3306/app", "app");
            state.connection_caches.save(
                &target_id,
                valid_mysql_cache(&target.dsn, target.database.as_deref().unwrap()),
            );

            let probe_effects = reduce(&mut state, &Action::SwitchConnection(target.clone()))
                .expect("switch should start a probe");
            let probe_run_id = probe_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target,
                    run_id: probe_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            assert!(!state.session.connection_state().is_connected());
            assert!(state.session.metadata().is_none());
            assert!(state.query.current_result().is_none());
        }

        #[test]
        fn sqlite_switch_restores_cache() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            let mut pagination = PaginationState::default();
            pagination.reset_for_table("main", "items");
            pagination.set_page_result(1, true);
            state.ui.set_explorer_selected_raw(7);
            state.connection_caches.save(
                &target_id,
                ConnectionCache {
                    explorer_selected: 42,
                    inspector_tab: InspectorTab::ForeignKeys,
                    pagination,
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
            assert_eq!(state.query.pagination.schema(), "main");
            assert_eq!(state.query.pagination.table(), "items");
            assert_eq!(state.query.pagination.current_page(), 1);
            assert!(state.query.pagination.reached_end());
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::SQLite)
            );
            assert_eq!(state.session.connection_state(), ConnectionState::Connected);
        }

        #[test]
        fn cached_round_trip_restores_pagination_target_for_next_and_previous_pages() {
            let mut state = AppState::new("test".to_string());
            let connection_a = ConnectionId::from_string("connection-a");
            let connection_b = ConnectionId::from_string("connection-b");
            let dsn_a = "postgres://localhost/connection-a";
            let dsn_b = "postgres://localhost/connection-b";

            state.session.activate_connection_with_dsn(
                &connection_a,
                "connection-a",
                DatabaseType::PostgreSQL,
                dsn_a,
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            state.query.pagination.reset_for_table("public", "users");
            state.query.pagination.set_page_result(2, false);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string()],
                    vec![vec!["1".to_string()]],
                    1500,
                    QuerySource::Preview,
                )));

            let mut pagination_b = PaginationState::default();
            pagination_b.reset_for_table("main", "orders");
            pagination_b.set_page_result(5, false);
            state.connection_caches.save(
                &connection_b,
                ConnectionCache {
                    selected_table_key: Some("main.orders".to_string()),
                    query_result: Some(Arc::new(QueryResult::success(
                        "SELECT * FROM orders".to_string(),
                        vec!["id".to_string()],
                        vec![vec!["2".to_string()]],
                        2600,
                        QuerySource::Preview,
                    ))),
                    pagination: pagination_b,
                    ..Default::default()
                },
            );

            reduce(
                &mut state,
                &create_postgres_switch_action(&connection_b, "connection-b"),
            );

            assert_eq!(state.session.dsn(), Some(dsn_b));
            assert_eq!(state.session.selected_table_key(), Some("main.orders"));
            assert_eq!(state.query.pagination.current_page(), 5);
            assert_eq!(state.query.pagination.schema(), "main");
            assert_eq!(state.query.pagination.table(), "orders");

            let next_effects = reduce_app(
                &mut state,
                Action::ResultNextPage,
                std::time::Instant::now(),
                &AppServices::stub(),
            );
            assert!(next_effects.iter().any(|effect| matches!(
                effect,
                Effect::ExecutePreview {
                    dsn,
                    schema,
                    table,
                    offset: 3000,
                    target_page: 6,
                    ..
                } if dsn == dsn_b && schema == "main" && table == "orders"
            )));

            state.query.mark_idle();
            let previous_effects = reduce_app(
                &mut state,
                Action::ResultPrevPage,
                std::time::Instant::now(),
                &AppServices::stub(),
            );
            assert!(previous_effects.iter().any(|effect| matches!(
                effect,
                Effect::ExecutePreview {
                    dsn,
                    schema,
                    table,
                    offset: 2000,
                    target_page: 4,
                    ..
                } if dsn == dsn_b && schema == "main" && table == "orders"
            )));

            reduce(
                &mut state,
                &create_postgres_switch_action(&connection_a, "connection-a"),
            );
            assert_eq!(state.session.dsn(), Some(dsn_a));
            assert_eq!(state.session.selected_table_key(), Some("public.users"));
            assert_eq!(state.query.pagination.current_page(), 2);
            assert_eq!(state.query.pagination.schema(), "public");
            assert_eq!(state.query.pagination.table(), "users");
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
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let action = create_postgres_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(!state.table_prefetch.has_pending_prefetch());
        }

        #[test]
        fn cached_switch_clears_pending_er_picker() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());

            let action = create_postgres_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(!state.table_prefetch.has_pending_prefetch());
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
                Effect::ProbeMySqlConnection { target: actual, .. } if actual.dsn == target.dsn
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("switch should include the probe run");

            let effects = reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: target.clone(),
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
            )
            .expect("probe completion should be handled");

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == "mysql://user@localhost:3306/app"
            )));
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);

            let metadata_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::FetchMetadata { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("probe completion should start metadata loading");
            let error_effects = reduce_app(
                &mut state,
                Action::MetadataFailed {
                    dsn: target.dsn,
                    run_id: metadata_run_id,
                    error: DbOperationError::ConnectionFailed("connection refused".to_string()),
                },
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.session.connection_state().is_failed());
            assert_eq!(state.modal.active_mode(), InputMode::ConnectionError);
            assert!(state.connection_error.error_info().is_some());
            assert!(matches!(
                error_effects.as_slice(),
                [Effect::CancelTrackedTasks]
            ));
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
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
                &Action::MySqlConnectionProbeCompleted {
                    target: mysql,
                    run_id: mysql_run_id,
                    lower_case_table_names: 0,
                },
            );

            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::MySQL)
            );
            assert!(state.session.connection_state().is_connecting());
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: mysql,
                    run_id: mysql_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            assert!(state.session.connection_state().is_connected());
            assert!(!state.session.is_reloading());
        }

        #[test]
        fn mysql_probe_failure_ends_loading_without_clearing_selection() {
            let mut state = AppState::new("test".to_string());
            let first = ConnectionTarget {
                id: ConnectionId::from_string("mysql-a"),
                dsn: "mysql://user@localhost:3306/a?ssl-mode=PREFERRED".to_string(),
                name: "mysql-a".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("a".to_string()),
            };
            state.session.activate_connection_with_target(
                &first.id,
                &first.name,
                first.database_type,
                &first.dsn,
                first.database.as_deref(),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let detail_run_id = state.session.begin_table_detail_run();

            let second = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(second.clone())).unwrap();
            let probe_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            let retry_effects = reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: second,
                    run_id: probe_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            )
            .unwrap();

            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.session.selected_table_key(), Some("public.users"));
            assert!(state.session.table_detail().is_none());
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Loading
            ));
            assert_eq!(state.session.selection_generation(), generation);
            assert!(!state.session.is_current_table_detail_run(detail_run_id));
            let retry_run_id = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::FetchTableDetail {
                        dsn,
                        schema,
                        table,
                        generation: effect_generation,
                        run_id,
                    } if dsn == &first.dsn
                        && schema == "public"
                        && table == "users"
                        && *effect_generation == generation =>
                    {
                        Some(*run_id)
                    }
                    _ => None,
                })
                .unwrap();
            assert_ne!(retry_run_id, detail_run_id);
            assert!(state.session.is_current_table_detail_run(retry_run_id));
        }

        #[test]
        fn mysql_probe_failure_preserves_loaded_inspector_result_and_cancels_query() {
            let mut state = AppState::new("test".to_string());
            let first = ConnectionTarget {
                id: ConnectionId::from_string("mysql-a"),
                dsn: "mysql://user@localhost:3306/a?ssl-mode=PREFERRED".to_string(),
                name: "mysql-a".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("a".to_string()),
            };
            state.session.activate_connection_with_target(
                &first.id,
                &first.name,
                first.database_type,
                &first.dsn,
                first.database.as_deref(),
            );
            state.session.mark_probe_connected();
            state.ui.set_explorer_selected_raw(3);
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let detail = Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: Vec::new(),
                primary_key: None,
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
                rls: None,
                triggers: Vec::new(),
                row_count_estimate: None,
                comment: None,
                source_ddl: None,
                storage_attributes: TableStorageAttributes::default(),
                kind_info: TableKindInfo::default(),
            };
            assert!(state.session.set_table_detail(detail, generation));
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string()],
                    vec![vec!["1".to_string()]],
                    10,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");
            state.query.pagination.set_current_page(2);
            let query_run_id = state.query.begin_running(std::time::Instant::now());

            let second = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let effects = reduce(&mut state, &Action::SwitchConnection(second.clone())).unwrap();
            let probe_run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            assert!(matches!(effects.first(), Some(Effect::CancelTrackedTasks)));
            assert!(!state.query.is_running());
            assert!(!state.query.is_current_run(query_run_id));

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: second,
                    run_id: probe_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            assert!(state.session.connection_state().is_connected());
            assert_eq!(state.session.selected_table_key(), Some("public.users"));
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Loaded(_)
            ));
            assert!(state.session.table_detail().is_some());
            assert!(state.query.current_result().is_some());
            assert_eq!(state.query.pagination.current_page(), 2);
            assert_eq!(state.ui.explorer_selected(), 3);
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(&mut state, &Action::SwitchConnection(second));

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target: first,
                    run_id: first_run_id,
                    lower_case_table_names: 0,
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(&mut state, &Action::SwitchConnection(second));

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
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
                    Effect::ProbeMySqlConnection { target, run_id } => Some((target, *run_id)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(retry_target.dsn, target.dsn);
            assert_eq!(retry_target.id, target.id);
            assert_ne!(retry_run_id, first_run_id);
        }

        #[test]
        fn switching_after_mysql_probe_failure_replaces_error_and_retry_target() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::from_string("mysql-a");
            state.session.activate_connection_with_target(
                &current_id,
                "mysql-a",
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
                Some("a"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            let failed_target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let failed_effects =
                reduce(&mut state, &Action::SwitchConnection(failed_target.clone())).unwrap();
            let failed_run_id = failed_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: failed_target,
                    run_id: failed_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            let retry_target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-c"),
                dsn: "mysql://user@localhost:3306/c?ssl-mode=PREFERRED".to_string(),
                name: "mysql-c".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("c".to_string()),
            };
            let retry_effects =
                reduce(&mut state, &Action::SwitchConnection(retry_target.clone())).unwrap();
            let retry_run_id = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            assert!(state.connection_error.error_info().is_none());

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: retry_target.clone(),
                    run_id: retry_run_id,
                    error: DbOperationError::ConnectionFailed("refused again".to_string()),
                },
            );
            let retry_effects = reduce_connection_error(
                &mut state,
                &Action::RetryConnection,
                std::time::Instant::now(),
            )
            .into_effects()
            .unwrap();
            let actual_target = retry_effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::ProbeMySqlConnection { target, .. } => Some(target),
                    _ => None,
                })
                .unwrap();
            assert_eq!(actual_target.id, retry_target.id);
            assert_eq!(actual_target.dsn, retry_target.dsn);
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
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
        fn postgres_reload_does_not_clear_failed_mysql_probe() {
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
                    Effect::ProbeMySqlConnection { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .unwrap();
            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target: mysql,
                    run_id: mysql_run_id,
                    error: DbOperationError::ConnectionFailed("refused".to_string()),
                },
            );

            let reload_effects = reduce_app(
                &mut state,
                Action::ReloadMetadata,
                std::time::Instant::now(),
                &AppServices::stub(),
            );

            assert!(reload_effects.is_empty());
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(mysql_run_id)
            );
            assert_eq!(
                state.messages.last_error(),
                Some("Connection switch in progress")
            );
        }

        #[test]
        fn mysql_switch_without_database_is_rejected_before_probe() {
            let mut state = AppState::new("test".to_string());
            let current = ConnectionTarget {
                id: ConnectionId::from_string("postgres-current"),
                dsn: "postgres://localhost/current".to_string(),
                name: "current".to_string(),
                database_type: DatabaseType::PostgreSQL,
                database: None,
            };
            reduce(&mut state, &Action::SwitchConnection(current.clone())).unwrap();
            state.connection_caches.save(
                &current.id,
                ConnectionCache {
                    explorer_selected: 7,
                    ..Default::default()
                },
            );
            let target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-old"),
                dsn: "mysql://user@localhost:3306".to_string(),
                name: "old mysql".to_string(),
                database_type: DatabaseType::MySQL,
                database: None,
            };

            let effects = reduce(&mut state, &Action::SwitchConnection(target)).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.session.active_connection_id(), Some(&current.id));
            assert_eq!(state.session.active_database(), None);
            assert_eq!(
                state
                    .connection_caches
                    .get(&current.id)
                    .unwrap()
                    .explorer_selected,
                7
            );
            assert!(state.messages.last_error().is_some_and(|message| {
                message.contains("MySQL connection field `database` is required")
            }));
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
            let run_id = state.session.begin_mysql_connection_probe(
                &target.id,
                &target.name,
                &target.dsn,
                target.database.as_deref(),
            );

            reduce(
                &mut state,
                &Action::MySqlConnectionProbeFailed {
                    target,
                    run_id,
                    error: DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::Auth,
                        details: "ERROR 1045: Access denied for user 'user'".to_string(),
                    },
                },
            );

            assert!(!state.session.connection_state().is_connected());
            assert!(state.session.connection_state().is_failed());
            assert_eq!(state.modal.active_mode(), InputMode::ConnectionError);
            assert_eq!(
                state.connection_error.error_info().unwrap().summary(),
                "Authentication failed"
            );
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

            let action = create_postgres_switch_action(&target_id, "cached_db");
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
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let action = create_postgres_switch_action(&target_id, "cached_db");
            reduce(&mut state, &action);

            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert!(!state.table_prefetch.has_pending_prefetch());
        }

        #[test]
        fn switch_without_cache_resets_sql_prefetch() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let action = create_postgres_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert!(!state.table_prefetch.has_pending_prefetch());
        }

        #[test]
        fn resets_result_selection_when_no_cache() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            state.result_interaction.activate_cell(5, 0);

            let action = create_postgres_switch_action(&new_id, "fresh_db");
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

            let action = create_postgres_switch_action(&new_id, "fresh_db");
            reduce(&mut state, &action);

            assert!(!state.session.is_read_only());
        }

        #[test]
        fn clears_completion_cache_on_switch() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = create_postgres_switch_action(&new_id, "any_db");
            let effects = reduce(&mut state, &action).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
        }
    }
}
