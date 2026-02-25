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
                "✓ Opened {} ({}/{} tables)",
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

            // reset() clears target_tables too, so save and restore it
            let target_tables = state.er_preparation.target_tables.clone();
            state.er_preparation.reset();
            state.er_preparation.target_tables = target_tables;

            state.sql_modal.prefetch_started = false;
            state.sql_modal.prefetch_queue.clear();
            state.sql_modal.prefetching_tables.clear();
            state.sql_modal.failed_prefetch_tables.clear();

            state.er_preparation.status = ErStatus::Waiting;
            state.set_success("Starting table prefetch for ER diagram...".to_string());

            // Always force a full refresh so schema changes are reflected.
            // StartPrefetchAll fires from MetadataLoaded when er status is Waiting.
            Some(vec![Effect::Sequence(vec![
                Effect::CacheInvalidate { dsn: dsn.clone() },
                Effect::ClearCompletionEngineCache,
                Effect::FetchMetadata { dsn },
            ])])
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

        #[test]
        fn stale_cache_returns_sequence_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.prefetch_started = true;

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::Sequence(seq)
                    if matches!(seq.as_slice(), [
                        Effect::CacheInvalidate { .. },
                        Effect::ClearCompletionEngineCache,
                        Effect::FetchMetadata { .. },
                    ])
            ));
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert!(!state.sql_modal.prefetch_started);
        }

        #[test]
        fn target_tables_survive_reset() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.target_tables =
                vec!["public.users".to_string(), "public.orders".to_string()];

            reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now());

            assert_eq!(
                state.er_preparation.target_tables,
                vec!["public.users".to_string(), "public.orders".to_string()]
            );
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
        fn no_dsn_returns_error() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce_er(&mut state, &Action::ErOpenDiagram, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }
    }
}
