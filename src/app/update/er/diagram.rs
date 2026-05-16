use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
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
            state.er_preparation.status = ErStatus::Idle;
            // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
            state.sql_modal.invalidate_prefetch();
            state.set_success(format!(
                "✓ Opened {path} ({table_count}/{total_tables} tables) — Stale? Press r to reload"
            ));
            DispatchResult::handled()
        }
        Action::ErDiagramFailed(error) => {
            state.er_preparation.status = ErStatus::Idle;
            state.set_error(error.to_string());
            DispatchResult::handled()
        }
        Action::ErLogWriteFailed(error) => {
            state.set_error(error.to_string());
            DispatchResult::handled()
        }
        Action::ErOpenDiagram => {
            if matches!(
                state.er_preparation.status,
                ErStatus::Rendering | ErStatus::Waiting
            ) {
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
            state.er_preparation.run_id += 1;
            state.er_preparation.status = ErStatus::Waiting;
            state.set_success("Checking for schema changes...".to_string());

            DispatchResult::handled_with(vec![Effect::SmartErRefresh {
                dsn,
                run_id: state.er_preparation.run_id,
            }])
        }
        Action::ErGenerateFromCache => {
            if !matches!(
                state.er_preparation.status,
                ErStatus::Idle | ErStatus::Waiting
            ) {
                return DispatchResult::handled();
            }

            state.er_preparation.status = ErStatus::Rendering;
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
