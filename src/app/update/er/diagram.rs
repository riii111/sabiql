use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, ErDiagramInfo};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::require_er_diagram_enabled;

pub(super) fn reduce_diagram_lifecycle(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ErDiagramOpened(ErDiagramInfo {
            path,
            table_count,
            total_tables,
        }) => {
            state.er_preparation.mark_idle();
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.sql_modal.invalidate_prefetch();
            state.messages.set_success_at(
                format!(
                    "✓ Opened {path} ({table_count}/{total_tables} tables) — Stale? Press r to reload"
                ),
                now,
            );
            DispatchResult::handled()
        }
        Action::ErDiagramFailed(error) => {
            state.er_preparation.mark_idle();
            state.messages.set_error_at(error.to_string(), now);
            DispatchResult::handled()
        }
        Action::ErLogWriteFailed(error) => {
            state.messages.set_error_at(error.to_string(), now);
            DispatchResult::handled()
        }
        Action::ErOpenDiagram => {
            if let Some(result) = require_er_diagram_enabled(state, now) {
                return result;
            }
            if state.er_preparation.is_busy() {
                return DispatchResult::handled();
            }

            let Some(dsn) = state.session.dsn().map(String::from) else {
                state
                    .messages
                    .set_error_at("No active connection".to_string(), now);
                return DispatchResult::handled();
            };
            if state.session.metadata().is_none() {
                state
                    .messages
                    .set_error_at("Metadata not loaded yet".to_string(), now);
                return DispatchResult::handled();
            }

            state.sql_modal.invalidate_prefetch();
            let run_id = state.er_preparation.start_waiting_run();
            state
                .messages
                .set_success_at("Checking for schema changes...".to_string(), now);

            DispatchResult::handled_with(vec![Effect::SmartErRefresh { dsn, run_id }])
        }
        Action::ErGenerateFromCache => {
            if let Some(result) = require_er_diagram_enabled(state, now) {
                return result;
            }
            if !state.er_preparation.can_generate_from_cache() {
                return DispatchResult::handled();
            }

            state.er_preparation.mark_rendering();
            let total_tables = state
                .session
                .metadata()
                .map_or(0, |m| m.table_summaries.len());

            DispatchResult::handled_with(vec![Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name: state.runtime.project_name.clone(),
                target_tables: state.er_preparation.target_tables().to_vec(),
            }])
        }
        _ => DispatchResult::pass(),
    }
}
