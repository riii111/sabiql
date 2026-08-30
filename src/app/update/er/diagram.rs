use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, ErDiagramInfo};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(super) fn reduce_diagram_lifecycle(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ErDiagramOpened(ErDiagramInfo {
            run_id,
            path,
            table_count,
            total_tables,
        }) => {
            if !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_idle();
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.table_prefetch.invalidate_prefetch();
            state.messages.set_success_at(
                format!(
                    "✓ Opened {path} ({table_count}/{total_tables} tables) — Stale? Press r to reload"
                ),
                now,
            );
            DispatchResult::handled()
        }
        Action::ErDiagramFailed { run_id, error } => {
            if !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_idle();
            state.messages.set_error(error.clone());
            DispatchResult::handled()
        }
        Action::ErLogWriteFailed(error) => {
            state.messages.set_error(error.clone());
            DispatchResult::handled()
        }
        Action::ErOpenDiagram => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if state.er_preparation.is_busy() {
                return DispatchResult::handled();
            }

            let Some(dsn) = state.session.dsn().map(String::from) else {
                state.messages.set_error("No active connection".to_string());
                return DispatchResult::handled();
            };
            if state.session.metadata().is_none() {
                state
                    .messages
                    .set_error("Metadata not loaded yet".to_string());
                return DispatchResult::handled();
            }

            state.table_prefetch.invalidate_prefetch();
            let run_id = state.er_preparation.start_waiting_run();
            state
                .messages
                .set_success_at("Checking for schema changes...".to_string(), now);

            DispatchResult::handled_with(vec![Effect::SmartErRefresh { dsn, run_id }])
        }
        Action::ErGenerateFromCache => {
            if !state.er_preparation.can_generate_from_cache() {
                return DispatchResult::handled();
            }

            state.er_preparation.mark_rendering();
            let run_id = state.er_preparation.run_id();
            let total_tables = state
                .session
                .metadata()
                .map_or(0, |m| m.table_summaries.len());

            DispatchResult::handled_with(vec![Effect::GenerateErDiagramFromCache {
                run_id,
                total_tables,
                project_name: state.runtime.project_name.clone(),
                target_tables: state.er_preparation.target_tables().to_vec(),
            }])
        }
        _ => DispatchResult::pass(),
    }
}
