mod diagram;
mod smart_refresh_completed;
mod smart_refresh_failed;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_er(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    diagram::reduce_diagram_lifecycle(state, action, now)
        .or_else(|| smart_refresh_completed::reduce_smart_refresh_completed(state, action, now))
        .or_else(|| smart_refresh_failed::reduce_smart_refresh_failed(state, action, now))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::model::app_state::AppState;
    use crate::model::er_state::ErStatus;
    use crate::update::action::{SmartErRefreshError, SmartErRefreshResult};
    use std::sync::Arc;

    fn reduce_er(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
        super::dispatch_er(state, action, now)
    }

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.dsn = Some(dsn.to_string());
        state
    }

    mod er_open_diagram {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(0)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(state.er_preparation.run_id, 1);
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
            state.er_preparation.run_id = 3;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.er_preparation.run_id, 4);
            assert!(matches!(
                &effects[0],
                Effect::SmartErRefresh { run_id: 4, .. }
            ));
        }

        #[test]
        fn prefetch_started_true_still_resets_and_emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state.session.set_metadata(Some(make_metadata(0)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(!state.sql_modal.is_prefetch_started());
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::SmartErRefresh { .. }));
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
            state.er_preparation.status = ErStatus::Rendering;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn waiting_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }

        #[test]
        fn no_metadata_returns_error() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }
    }

    mod er_generate_from_cache {
        use super::*;
        use crate::domain::DatabaseMetadata;

        #[test]
        fn idle_status_returns_generate_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Idle;
            state.session.set_metadata(Some(Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: vec![],
                fetched_at: Instant::now(),
            })));
            state.er_preparation.target_tables = vec!["public.users".to_string()];

            let effects = reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::GenerateErDiagramFromCache { target_tables, .. }
                    if target_tables == &vec!["public.users".to_string()]
            ));
            assert_eq!(state.er_preparation.status, ErStatus::Rendering);
        }

        #[test]
        fn rendering_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Rendering;

            let effects = reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
        }
    }

    mod smart_er_refresh_completed {
        use super::*;
        use std::collections::HashMap;

        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn no_changes_dispatches_generate_from_cache() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec![],
                added_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn stale_tables_trigger_evict_and_scoped_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec!["public.users".to_string()],
                added_tables: vec![],
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
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn added_tables_trigger_scoped_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(3),
                stale_tables: vec![],
                added_tables: vec!["public.new_table".to_string()],
                removed_tables: vec![],
                missing_in_cache: vec![],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn removed_tables_trigger_evict() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(1),
                stale_tables: vec![],
                added_tables: vec![],
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
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(2),
                stale_tables: vec![],
                added_tables: vec![],
                removed_tables: vec![],
                missing_in_cache: vec!["public.uncached".to_string()],
                new_signatures: HashMap::new(),
            });

            let effects = reduce_er(&mut state, &action, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn mismatched_run_id_returns_empty_for_completed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 5;
            state.er_preparation.status = ErStatus::Waiting;

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 3,
                new_metadata: make_metadata(0),
                stale_tables: vec![],
                added_tables: vec![],
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
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let new_sigs: HashMap<String, String> =
                std::iter::once(("public.users".to_string(), "abc123".to_string())).collect();

            let action = Action::SmartErRefreshCompleted(SmartErRefreshResult {
                run_id: 1,
                new_metadata: make_metadata(5),
                stale_tables: vec![],
                added_tables: vec![],
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
            assert_eq!(state.er_preparation.last_signatures, new_sigs);
        }
    }

    mod smart_er_refresh_failed {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};
        use crate::ports::outbound::DbOperationError;

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn falls_back_to_full_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(5)));
            state
                .er_preparation
                .last_signatures
                .insert("public.old".to_string(), "sig".to_string());

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.last_signatures.is_empty());
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
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchAll))
            )));
        }

        #[test]
        fn falls_back_to_scoped_prefetch_when_targets_set() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(10)));
            state.er_preparation.target_tables = vec!["public.t0".to_string()];

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
            assert!(state.er_preparation.last_signatures.is_empty());
            assert!(
                state
                    .messages
                    .last_error
                    .as_deref()
                    .is_some_and(|message| message.contains("falling back to scoped prefetch"))
            );
        }

        #[test]
        fn mismatched_run_id_returns_empty_for_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 5;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
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
        fn no_metadata_sets_idle_and_error() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id: 1,
                    error: DbOperationError::Timeout("timed out".to_string()),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn new_metadata_applied_before_fallback() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(3)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
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
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchAll))
            )));
        }
    }
}
