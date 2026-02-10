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

            if !state.sql_modal.prefetch_started
                && let Some(metadata) = &state.cache.metadata
            {
                state.er_preparation.total_tables = metadata.tables.len();
                state.er_preparation.status = ErStatus::Waiting;
                state.set_success("Starting table prefetch for ER diagram...".to_string());
                return Some(vec![Effect::DispatchActions(vec![
                    Action::StartPrefetchAll,
                ])]);
            }

            if state.cache.metadata.is_none() {
                state.set_error("Metadata not loaded yet".to_string());
                return Some(vec![]);
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
                target_table: state.er_preparation.target_table.clone(),
            }])
        }

        _ => None,
    }
}
