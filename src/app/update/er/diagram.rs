use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, ErDiagramInfo};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_diagram_lifecycle(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
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
            state.set_success(format!(
                "✓ Opened {path} ({table_count}/{total_tables} tables) — Stale? Press r to reload"
            ));
            DispatchResult::handled()
        }
        Action::ErDiagramFailed(error) => {
            state.er_preparation.mark_idle();
            state.set_error(error.to_string());
            DispatchResult::handled()
        }
        Action::ErLogWriteFailed(error) => {
            state.set_error(error.to_string());
            DispatchResult::handled()
        }
        Action::ErOpenDiagram => {
            if state.er_preparation.is_busy() {
                return DispatchResult::handled();
            }

            let Some(dsn) = state.session.dsn.clone() else {
                state.set_error("No active connection".to_string());
                return DispatchResult::handled();
            };
            if state.session.metadata().is_none() {
                state.set_error("Metadata not loaded yet".to_string());
                return DispatchResult::handled();
            }

            state.sql_modal.invalidate_prefetch();
            let run_id = state.er_preparation.begin_smart_refresh();
            state.set_success("Checking for schema changes...".to_string());

            DispatchResult::handled_with(vec![Effect::SmartErRefresh { dsn, run_id }])
        }
        Action::ErGenerateFromCache => {
            if !state.er_preparation.can_generate_from_cache() {
                return DispatchResult::handled();
            }

            state.er_preparation.begin_rendering();
            let total_tables = state
                .session
                .metadata()
                .map_or(0, |m| m.table_summaries.len());

            DispatchResult::handled_with(vec![Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name: state.runtime.project_name.clone(),
                target_tables: state.er_preparation.target_tables.clone(),
            }])
        }
        _ => DispatchResult::pass(),
    }
}
