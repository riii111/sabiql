//! ER diagram sub-reducer.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::er_state::ErStatus;
use crate::app::state::AppState;

/// Handles ER diagram actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_er(state: &mut AppState, action: &Action, _now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ErDiagramOpened {
            path,
            table_count,
            total_tables,
        } => {
            state.er_preparation.status = ErStatus::Idle;
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.sql_modal.prefetch_started = false;
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
        Action::ErOpenDiagram => {
            if matches!(
                state.er_preparation.status,
                ErStatus::Rendering | ErStatus::Waiting
            ) {
                return Some(vec![]);
            }

            let Some(dsn) = state.runtime.dsn.clone() else {
                state.set_error("No active connection".to_string());
                return Some(vec![]);
            };
            if state.cache.metadata.is_none() {
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
            }

            state.sql_modal.prefetch_started = false;
            state.er_preparation.run_id += 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.set_success("Checking for schema changes...".to_string());

            Some(vec![Effect::SmartErRefresh {
                dsn,
                run_id: state.er_preparation.run_id,
            }])
        }

        Action::SmartErRefreshCompleted {
            run_id,
            new_metadata,
            stale_tables,
            added_tables,
            removed_tables,
            missing_in_cache,
            new_signatures,
        } => {
            if *run_id != state.er_preparation.run_id {
                return Some(vec![]);
            }

            state.cache.metadata = Some(*new_metadata.clone());
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

        Action::SmartErRefreshFailed(_error) => {
            // Fallback: full cache clear + re-prefetch (legacy behavior)
            let Some(metadata) = &state.cache.metadata else {
                state.er_preparation.status = ErStatus::Idle;
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
            };
            let total_table_count = metadata.tables.len();
            let is_scoped = !state.er_preparation.target_tables.is_empty()
                && state.er_preparation.target_tables.len() < total_table_count;

            state.er_preparation.total_tables = total_table_count;

            if is_scoped {
                let scoped_tables = state.er_preparation.target_tables.clone();
                state.set_success(
                    "Fallback: starting scoped prefetch for ER diagram...".to_string(),
                );
                Some(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                        tables: scoped_tables,
                    }]),
                ])
            } else {
                state.set_success("Fallback: starting full prefetch for ER diagram...".to_string());
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
                .cache
                .metadata
                .as_ref()
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
        state.runtime.dsn = Some(dsn.to_string());
        state
    }

    mod er_open_diagram {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> DatabaseMetadata {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{}", i), "public".to_string(), None, false))
                .collect();
            DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            }
        }

        #[test]
        fn emits_smart_refresh() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.cache.metadata = Some(make_metadata(0));

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
            state.cache.metadata = Some(make_metadata(5));
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
            state.sql_modal.prefetch_started = true;
            state.cache.metadata = Some(make_metadata(0));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(!state.sql_modal.prefetch_started);
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::SmartErRefresh { .. }));
        }

        #[test]
        fn no_dsn_returns_error() {
            let mut state = AppState::new("test".to_string());
            state.cache.metadata = Some(make_metadata(5));

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
            state.sql_modal.prefetch_started = true;

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
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
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
}
