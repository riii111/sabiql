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

            let Some(metadata) = &state.cache.metadata else {
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
            };
            let total_table_count = metadata.tables.len();
            let is_scoped = !state.er_preparation.target_tables.is_empty()
                && state.er_preparation.target_tables.len() < total_table_count;

            if !state.sql_modal.prefetch_started {
                state.er_preparation.total_tables = total_table_count;
                state.er_preparation.status = ErStatus::Waiting;

                if is_scoped {
                    let scoped_tables = state.er_preparation.target_tables.clone();
                    state.set_success("Starting scoped prefetch for ER diagram...".to_string());
                    return Some(vec![Effect::DispatchActions(vec![
                        Action::StartPrefetchScoped {
                            tables: scoped_tables,
                        },
                    ])]);
                } else {
                    state.set_success("Starting table prefetch for ER diagram...".to_string());
                    return Some(vec![Effect::DispatchActions(vec![
                        Action::StartPrefetchAll,
                    ])]);
                }
            }

            if state.er_preparation.has_failures() {
                let failed_tables: Vec<String> =
                    state.er_preparation.failed_tables.keys().cloned().collect();
                state.er_preparation.retry_failed();
                state.sql_modal.failed_prefetch_tables.clear();

                for qualified_name in failed_tables {
                    state.sql_modal.prefetch_queue.push_back(qualified_name);
                }

                state.er_preparation.status = ErStatus::Waiting;
                return Some(vec![Effect::ProcessPrefetchQueue]);
            }

            if !state.er_preparation.is_complete() {
                state.er_preparation.status = ErStatus::Waiting;
                return Some(vec![]);
            }

            // Prefetch already complete — delegate to ErGenerateFromCache
            Some(vec![Effect::DispatchActions(vec![
                Action::ErGenerateFromCache,
            ])])
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
        fn no_prefetch_starts_prefetch_all() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.cache.metadata = Some(make_metadata(0));

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], Effect::DispatchActions(_)));
        }

        #[test]
        fn target_tables_non_empty_dispatches_scoped() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.cache.metadata = Some(make_metadata(100));
            state.er_preparation.target_tables =
                vec!["public.t0".to_string(), "public.t1".to_string()];

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchScoped { .. }))
            )));
        }

        #[test]
        fn target_tables_empty_dispatches_prefetch_all() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.cache.metadata = Some(make_metadata(10));
            // target_tables is empty → StartPrefetchAll

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::StartPrefetchAll))
            )));
        }

        #[test]
        fn prefetch_complete_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.prefetch_started = true;
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            ));
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
