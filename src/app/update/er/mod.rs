mod diagram;
mod smart_refresh_completed;
mod smart_refresh_failed;
mod smart_refresh_fetched;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(crate) fn dispatch_er(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    if matches!(
        action,
        Action::ErGenerateFromCache
            | Action::ErDiagramOpened(_)
            | Action::ErDiagramFailed { .. }
            | Action::SmartErRefreshFetched(_)
            | Action::SmartErRefreshCompleted(_)
            | Action::SmartErRefreshFailed(_)
    ) && reject_pending_mysql_connection_probe(state)
    {
        return DispatchResult::handled();
    }

    diagram::reduce_diagram_lifecycle(state, action, now)
        .or_else(|| smart_refresh_fetched::reduce_smart_refresh_fetched(state, action))
        .or_else(|| smart_refresh_completed::reduce_smart_refresh_completed(state, action, now))
        .or_else(|| smart_refresh_failed::reduce_smart_refresh_failed(state, action))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Instant;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::domain::{
        ConnectionId, DatabaseMetadata, DatabaseType, TableSignatureSnapshot, TableSummary,
    };
    use crate::model::app_state::AppState;
    use crate::model::er_state::ErStatus;
    use crate::update::action::{
        ErDiagramInfo, SmartErRefreshError, SmartErRefreshFetched, SmartErRefreshResult,
    };
    use std::sync::Arc;

    fn reduce_er(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
        super::dispatch_er(state, action, now)
    }

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "postgres",
            DatabaseType::PostgreSQL,
            dsn,
        );
        state
    }

    fn state_with_mysql_connection() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_target(
            &ConnectionId::from_string("mysql-current"),
            "mysql-current",
            DatabaseType::MySQL,
            "mysql://localhost/current",
            Some("current"),
        );
        state
    }

    fn state_with_pending_mysql_probe() -> AppState {
        let mut state = state_with_mysql_connection();
        let _ = state.session.begin_mysql_connection_probe(
            &ConnectionId::from_string("mysql-target"),
            "mysql-target",
            "mysql://localhost/target",
            Some("target"),
        );
        state
    }

    fn set_active_run_id(state: &mut AppState, run_id: u64) {
        for _ in 0..run_id {
            let _ = state.er_preparation.start_waiting_run();
        }
        state.er_preparation.mark_idle();
    }

    fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
        let tables: Vec<TableSummary> = (0..table_count)
            .map(|i| TableSummary::new("public".to_string(), format!("t{i}"), None, false))
            .collect();
        Arc::new({
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.table_summaries = tables;
            metadata
        })
    }

    mod er_open_diagram {
        use super::*;
        use crate::ports::outbound::DbOperationError;
        use crate::services::AppServices;
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer;

        #[test]
        fn emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(0)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
            assert_eq!(state.er_preparation.run_id(), 1);
            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::SmartErRefresh { run_id: 1, .. }
            ));
        }

        #[test]
        fn increments_run_id_on_each_call() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(5)));
            set_active_run_id(&mut state, 3);

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.er_preparation.run_id(), 4);
            assert!(matches!(
                &effects[0],
                Effect::SmartErRefresh { run_id: 4, .. }
            ));
        }

        #[test]
        fn active_prefetch_run_still_resets_and_emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.table_prefetch.begin_er_prefetch();
            state.session.set_metadata(Some(make_metadata(0)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::SmartErRefresh { .. }));
        }

        #[test]
        fn pending_mysql_probe_rejects_er_open_before_prefetch_invalidation() {
            let mut state = state_with_pending_mysql_probe();
            state.session.set_metadata(Some(make_metadata(1)));
            let prefetch_run_id = state.table_prefetch.begin_er_prefetch();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("ER open action should be handled");

            assert!(effects.is_empty());
            assert_eq!(
                state.table_prefetch.active_prefetch_run_id(),
                Some(prefetch_run_id)
            );
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Connection switch in progress")
            );
        }

        #[test]
        fn pending_mysql_probe_rejects_er_completions_without_state_mutation() {
            let mut state = state_with_mysql_connection();
            state.session.set_metadata(Some(make_metadata(1)));
            let run_id = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_waiting_for_test();
            let prefetch_run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.session.begin_mysql_connection_probe(
                &ConnectionId::from_string("mysql-target"),
                "mysql-target",
                "mysql://localhost/target",
                Some("target"),
            );

            let actions = vec![
                Action::SmartErRefreshFetched(SmartErRefreshFetched {
                    dsn: "mysql://localhost/current".to_string(),
                    run_id,
                    new_metadata: make_metadata(1),
                    signature_snapshot: Arc::new(TableSignatureSnapshot {
                        signatures: vec![],
                        prefetched_table_details: vec![],
                    }),
                }),
                Action::SmartErRefreshCompleted(SmartErRefreshResult {
                    dsn: "mysql://localhost/current".to_string(),
                    run_id,
                    new_metadata: make_metadata(1),
                    stale_tables: vec![],
                    removed_tables: vec![],
                    missing_in_cache: vec![],
                    new_signatures: HashMap::new(),
                }),
                Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "mysql://localhost/current".to_string(),
                    run_id,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Action::ErDiagramOpened(ErDiagramInfo {
                    run_id,
                    path: "diagram.svg".to_string(),
                    table_count: 1,
                    total_tables: 1,
                }),
                Action::ErDiagramFailed {
                    run_id,
                    error: "failed".to_string(),
                },
                Action::ErGenerateFromCache,
            ];

            for action in actions {
                let effects = reduce_er(&mut state, &action, Instant::now())
                    .into_effects()
                    .expect("ER action should be handled");
                assert!(effects.is_empty());
            }

            assert_eq!(
                state.table_prefetch.active_prefetch_run_id(),
                Some(prefetch_run_id)
            );
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
        }

        #[test]
        fn mysql_probe_failure_cleans_er_run_for_retry() {
            let mut state = state_with_mysql_connection();
            state.session.set_metadata(Some(make_metadata(1)));
            let run_id = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_waiting_for_test();
            let _ = state.table_prefetch.begin_er_prefetch();
            let _ = state.session.begin_mysql_connection_probe(
                &ConnectionId::from_string("mysql-target"),
                "mysql-target",
                "mysql://localhost/target",
                Some("target"),
            );

            let effects = reduce_er(
                &mut state,
                &Action::ErDiagramOpened(ErDiagramInfo {
                    run_id,
                    path: "diagram.svg".to_string(),
                    table_count: 1,
                    total_tables: 1,
                }),
                Instant::now(),
            )
            .into_effects()
            .expect("ER completion should be handled");
            assert!(effects.is_empty());
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);

            let probe_run_id = state
                .session
                .pending_mysql_connection_probe()
                .expect("probe should still be pending")
                .run_id;
            let effects = reducer::reduce(
                &mut state,
                Action::MySqlConnectionProbeFailed {
                    target: ConnectionTarget {
                        id: ConnectionId::from_string("mysql-target"),
                        name: "mysql-target".to_string(),
                        dsn: "mysql://localhost/target".to_string(),
                        database_type: DatabaseType::MySQL,
                        database: Some("target".to_string()),
                    },
                    run_id: probe_run_id,
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(effects.is_empty());
            assert!(state.session.pending_mysql_connection_probe().is_some());
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);

            let effects = reducer::reduce(
                &mut state,
                Action::CloseConnectionError,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(matches!(effects.as_slice(), [Effect::CancelConnectionTask]));
            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(state.table_prefetch.active_prefetch_run_id().is_none());

            let effects = reducer::reduce(
                &mut state,
                Action::ErOpenDiagram,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(matches!(
                effects.as_slice(),
                [Effect::SmartErRefresh { run_id: reopened_run_id, .. }]
                    if *reopened_run_id == run_id + 1
            ));
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
        }

        #[test]
        fn no_dsn_returns_error() {
            let mut state = AppState::new("test".to_string());
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn rendering_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_rendering();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn waiting_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_waiting_for_test();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn no_metadata_returns_error() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.table_prefetch.begin_er_prefetch();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }
    }

    mod er_generate_from_cache {
        use super::*;

        #[test]
        fn idle_status_returns_generate_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_idle();
            state
                .session
                .set_metadata(Some(Arc::new(DatabaseMetadata::new("test".to_string()))));
            state
                .er_preparation
                .set_targets(vec!["public.users".to_string()]);

            let effects = reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::GenerateErDiagramFromCache { target_tables, .. }
                    if target_tables == &vec!["public.users".to_string()]
            ));
            assert_eq!(state.er_preparation.status(), ErStatus::Rendering);
        }

        #[test]
        fn rendering_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_rendering();

            let effects = reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }
    }

    mod stale_diagram_completion {
        use super::*;

        #[test]
        fn completion_after_reset_is_ignored() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_rendering();
            state.er_preparation.reset();

            let effects = reduce_er(
                &mut state,
                &Action::ErDiagramOpened(ErDiagramInfo {
                    run_id,
                    path: "stale.svg".to_string(),
                    table_count: 1,
                    total_tables: 1,
                }),
                Instant::now(),
            )
            .into_effects()
            .expect("reducer should handle stale completion");

            assert!(effects.is_empty());
            assert!(state.messages.last_success().is_none());
            assert!(state.messages.last_error().is_none());
        }
    }

    mod smart_er_refresh_completed {
        use super::*;

        #[test]
        fn no_changes_dispatches_generate_from_cache() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::GenerateErDiagramFromCache { .. }))
            );
        }

        #[test]
        fn stale_tables_trigger_evict_and_scoped_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec!["public.users".to_string()],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::EvictTablesFromCompletionCache { .. }))
            );
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartErPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn removed_tables_trigger_evict() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(1),
                stale_tables: vec![],
                removed_tables: vec!["public.dropped".to_string()],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::EvictTablesFromCompletionCache { tables }
                    if tables.contains(&"public.dropped".to_string())
            )));
        }

        #[test]
        fn missing_in_cache_triggers_scoped_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec!["public.uncached".to_string()],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            let Some(Effect::DispatchActions(actions)) = effects.iter().find(|effect| {
                matches!(effect, Effect::DispatchActions(actions) if actions.iter().any(
                    |action| matches!(action, Action::StartErPrefetchScoped { .. })
                ))
            }) else {
                panic!("expected scoped prefetch");
            };
            assert!(matches!(
                actions.as_slice(),
                [Action::StartErPrefetchScoped { tables }] if tables == &["public.uncached"]
            ));
        }

        #[test]
        fn stale_and_missing_tables_are_prefetched_once() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec!["public.uncached".to_string()],
                removed_tables: vec![],
                missing_in_cache: vec!["public.uncached".to_string()],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            let Some(Effect::DispatchActions(actions)) = effects.iter().find(|effect| {
                matches!(effect, Effect::DispatchActions(actions) if actions.iter().any(
                    |action| matches!(action, Action::StartErPrefetchScoped { .. })
                ))
            }) else {
                panic!("expected scoped prefetch");
            };
            assert!(matches!(
                actions.as_slice(),
                [Action::StartErPrefetchScoped { tables }] if tables == &["public.uncached"]
            ));
        }

        #[test]
        fn mismatched_run_id_returns_empty_for_completed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 5);
            state.er_preparation.mark_waiting_for_test();

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 3,
                new_metadata: make_metadata(0),
                stale_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn updates_metadata_and_signatures() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(0)));

            let new_sigs: HashMap<String, String> =
                std::iter::once(("public.users".to_string(), "abc123".to_string())).collect();

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                dsn: "postgres://localhost/test".to_string(),
                run_id: 1,
                new_metadata: make_metadata(5),
                stale_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: new_sigs.clone(),
            });

            reduce_er(&mut state, &action, Instant::now());

            assert_eq!(
                state
                    .session
                    .metadata()
                    .as_ref()
                    .unwrap()
                    .table_summaries
                    .len(),
                5
            );
            assert_eq!(state.er_preparation.last_signatures(), &new_sigs);
        }
    }

    mod smart_er_refresh_fetched {
        use super::*;

        fn action(dsn: &str, run_id: u64) -> Action {
            Action::SmartErRefreshFetched(SmartErRefreshFetched {
                dsn: dsn.to_string(),
                run_id,
                new_metadata: make_metadata(1),
                signature_snapshot: Arc::new(TableSignatureSnapshot {
                    signatures: Vec::new(),
                    prefetched_table_details: Vec::new(),
                }),
            })
        }

        #[test]
        fn matching_connection_and_run_emits_cache_diff_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();

            let effects = reduce_er(
                &mut state,
                &action("postgres://localhost/test", 1),
                Instant::now(),
            )
            .into_effects()
            .expect("reducer should handle action");

            assert!(matches!(
                &effects[0],
                Effect::SmartErRefreshCacheAndDiff { run_id: 1, .. }
            ));
        }

        #[test]
        fn mismatched_connection_or_run_does_not_emit_cache_diff_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 2);
            state.er_preparation.mark_waiting_for_test();

            assert!(
                reduce_er(
                    &mut state,
                    &action("postgres://localhost/test", 1),
                    Instant::now(),
                )
                .into_effects()
                .expect("reducer should handle action")
                .is_empty()
            );

            assert!(
                reduce_er(
                    &mut state,
                    &action("postgres://localhost/other", 2),
                    Instant::now(),
                )
                .into_effects()
                .expect("reducer should handle action")
                .is_empty()
            );
        }
    }

    mod smart_er_refresh_failed {
        use super::*;
        use crate::ports::outbound::DbOperationError;

        #[test]
        fn falls_back_to_full_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(5)));
            state.er_preparation.apply_refresh_metadata(
                HashMap::from([("public.old".to_string(), "sig".to_string())]),
                5,
            );

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.last_signatures().is_empty());
            assert!(state.messages.last_error.is_some());
            assert!(
                state
                    .messages
                    .last_error
                    .as_deref()
                    .is_some_and(|message| message.contains("falling back to full refresh"))
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartErPrefetchAll))
            )));
        }

        #[test]
        fn mismatched_run_id_returns_empty_for_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 5);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: 3,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn failure_after_er_state_reset_does_not_update_message_or_status() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.er_preparation.start_waiting_run();
            state.er_preparation.reset();
            let before_status = state.er_preparation.status();

            let effects = reduce_er(
                &mut state,
                &Action::ErDiagramFailed {
                    run_id,
                    error: "stale failure".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.er_preparation.status(), before_status);
            assert!(state.messages.last_error.is_none());
        }

        #[test]
        fn mismatched_dsn_returns_empty_for_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "postgres://other/test".to_string(),
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
            assert_eq!(state.session.metadata().unwrap().table_summaries.len(), 5);
            assert!(state.messages.last_error.is_none());
        }

        #[test]
        fn no_metadata_sets_idle_and_error() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn new_metadata_applied_before_fallback() {
            let mut state = state_with_dsn("postgres://localhost/test");
            set_active_run_id(&mut state, 1);
            state.er_preparation.mark_waiting_for_test();
            state.session.set_metadata(Some(make_metadata(3)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: 1,
                    error: DbOperationError::QueryFailed("sig fetch failed".to_string()),
                    new_metadata: Some(make_metadata(20)),
                }),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(
                state
                    .session
                    .metadata()
                    .as_ref()
                    .unwrap()
                    .table_summaries
                    .len(),
                20
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartErPrefetchAll))
            )));
        }
    }
}
