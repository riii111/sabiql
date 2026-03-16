use std::sync::Arc;
use std::time::Instant;

use crate::app::action::{Action, ErDiagramInfo, SmartErRefreshError, SmartErRefreshResult};
use crate::app::effect::Effect;
use crate::app::er_state::ErStatus;
use crate::app::state::AppState;

pub fn reduce_er(state: &mut AppState, action: &Action, _now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ErDiagramOpened(ErDiagramInfo {
            path,
            table_count,
            total_tables,
        }) => {
            state.er_preparation.status = ErStatus::Idle;
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.sql_modal.invalidate_prefetch();
            state.set_success(format!(
                "✓ Opened {} ({}/{} tables) — Stale? Press r to reload",
                path, table_count, total_tables
            ));
            Some(vec![])
        }
        Action::ErDiagramFailed(error) => {
            state.er_preparation.status = ErStatus::Idle;
            state.set_error(error.clone());
            Some(vec![])
        }
        Action::ErLogWriteFailed(error) => {
            state.set_error(error.clone());
            Some(vec![])
        }
        Action::ErOpenDiagram => {
            if matches!(
                state.er_preparation.status,
                ErStatus::Rendering | ErStatus::Waiting
            ) {
                return Some(vec![]);
            }

            let Some(dsn) = state.session.dsn.clone() else {
                state.set_error("No active connection".to_string());
                return Some(vec![]);
            };
            if state.session.metadata().is_none() {
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
            }

            state.sql_modal.invalidate_prefetch();
            state.er_preparation.run_id += 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.set_success("Checking for schema changes...".to_string());

            Some(vec![Effect::SmartErRefresh {
                dsn,
                run_id: state.er_preparation.run_id,
            }])
        }

        Action::SmartErRefreshCompleted(SmartErRefreshResult {
            run_id,
            new_metadata,
            stale_tables,
            added_tables,
            removed_tables,
            missing_in_cache,
            new_signatures,
        }) => {
            if *run_id != state.er_preparation.run_id {
                return Some(vec![]);
            }

            state.session.set_metadata(Some(Arc::clone(new_metadata)));
            state.er_preparation.last_signatures = new_signatures.clone();
            state.er_preparation.total_tables = new_metadata.tables.len();

            let mut effects: Vec<Effect> = Vec::new();

            if !removed_tables.is_empty() {
                effects.push(Effect::EvictTablesFromCompletionCache {
                    tables: removed_tables.clone(),
                });
            }

            let mut refetch: Vec<String> = Vec::new();
            refetch.extend(stale_tables.iter().cloned());
            refetch.extend(added_tables.iter().cloned());
            refetch.extend(missing_in_cache.iter().cloned());
            refetch.sort();
            refetch.dedup();

            if refetch.is_empty() {
                state.set_success(
                    "No schema changes detected, generating ER diagram...".to_string(),
                );
                effects.push(Effect::DispatchActions(vec![Action::ErGenerateFromCache]));
            } else {
                if !stale_tables.is_empty() {
                    effects.push(Effect::EvictTablesFromCompletionCache {
                        tables: stale_tables.clone(),
                    });
                }
                state.set_success(format!(
                    "Refreshing {} table(s) for ER diagram...",
                    refetch.len()
                ));
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                    tables: refetch,
                }]));
            }

            Some(effects)
        }

        Action::SmartErRefreshFailed(SmartErRefreshError {
            run_id,
            error,
            new_metadata,
        }) => {
            if *run_id != state.er_preparation.run_id {
                return Some(vec![]);
            }

            if let Some(md) = new_metadata {
                state.session.set_metadata(Some(Arc::clone(md)));
            }

            let Some(metadata) = &state.session.metadata() else {
                state.er_preparation.status = ErStatus::Idle;
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
            };
            let total_table_count = metadata.tables.len();
            let is_scoped = !state.er_preparation.target_tables.is_empty()
                && state.er_preparation.target_tables.len() < total_table_count;

            state.er_preparation.total_tables = total_table_count;
            state.er_preparation.last_signatures.clear();
            state.set_error(format!(
                "Smart refresh failed ({}), falling back to full refresh",
                error
            ));

            if is_scoped {
                let scoped_tables = state.er_preparation.target_tables.clone();
                Some(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                        tables: scoped_tables,
                    }]),
                ])
            } else {
                Some(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchAll]),
                ])
            }
        }

        Action::ErGenerateFromCache => {
            if !matches!(
                state.er_preparation.status,
                ErStatus::Idle | ErStatus::Waiting
            ) {
                return Some(vec![]);
            }

            state.er_preparation.status = ErStatus::Rendering;
            let total_tables = state
                .session
                .metadata()
                .map(|m| m.tables.len())
                .unwrap_or(0);

            Some(vec![Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name: state.runtime.project_name.clone(),
                target_tables: state.er_preparation.target_tables.clone(),
            }])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::app::state::AppState;

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
                .map(|i| TableSummary::new(format!("t{}", i), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(0)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(!state.sql_modal.is_prefetch_started());
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::SmartErRefresh { .. }));
        }

        #[test]
        fn no_dsn_returns_error() {
            let mut state = AppState::new("test".to_string());
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn rendering_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Rendering;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn waiting_status_returns_empty_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn no_metadata_returns_error() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

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
                tables: vec![],
                fetched_at: Instant::now(),
            })));
            state.er_preparation.target_tables = vec!["public.users".to_string()];

            let effects =
                reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now()).unwrap();

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

            let effects =
                reduce_er(&mut state, &Action::ErGenerateFromCache, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }
    }

    mod smart_er_refresh_completed {
        use super::*;
        use std::collections::HashMap;

        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{}", i), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn mismatched_run_id_returns_empty() {
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

            let effects = reduce_er(&mut state, &action, Instant::now()).unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn updates_metadata_and_signatures() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(0)));

            let new_sigs: HashMap<String, String> =
                [("public.users".to_string(), "abc123".to_string())]
                    .into_iter()
                    .collect();

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

            assert_eq!(state.session.metadata().as_ref().unwrap().tables.len(), 5);
            assert_eq!(state.er_preparation.last_signatures, new_sigs);
        }
    }

    mod smart_er_refresh_failed {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{}", i), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
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
                    error: "timeout".to_string(),
                    new_metadata: None,
                }),
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.last_signatures.is_empty());
            assert!(state.messages.last_error.is_some());
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
                    error: "timeout".to_string(),
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
        }

        #[test]
        fn mismatched_run_id_returns_empty() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.run_id = 5;
            state.er_preparation.status = ErStatus::Waiting;
            state.session.set_metadata(Some(make_metadata(5)));

            let effects = reduce_er(
                &mut state,
                &Action::SmartErRefreshFailed(SmartErRefreshError {
                    run_id: 3,
                    error: "timeout".to_string(),
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
                    error: "timeout".to_string(),
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
                    error: "sig fetch failed".to_string(),
                    new_metadata: Some(make_metadata(20)),
                }),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.session.metadata().as_ref().unwrap().tables.len(), 20);
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
