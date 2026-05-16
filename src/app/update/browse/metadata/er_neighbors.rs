use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::check_er_completion;

pub(super) fn reduce_er_neighbors(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::ExpandPrefetchWithFkNeighbors => {
            let seed_tables = state.er_preparation.seed_tables.clone();
            DispatchResult::handled_with(vec![Effect::ExtractFkNeighbors { seed_tables }])
        }
        Action::FkNeighborsDiscovered { tables } => {
            state.er_preparation.fk_expanded = true;

            if tables.is_empty() {
                // No new neighbors — proceed to generate with what we have
                return DispatchResult::handled_with(check_er_completion(state));
            }

            for qualified_name in tables {
                state
                    .er_preparation
                    .pending_tables
                    .insert(qualified_name.clone());
                state
                    .sql_modal
                    .prefetch_queue
                    .push_back(qualified_name.clone());
            }
            let Some(run_id) = state.sql_modal.active_prefetch_run_id() else {
                return DispatchResult::handled();
            };
            DispatchResult::handled_with(vec![Effect::ProcessPrefetchQueue { run_id }])
        }
        _ => DispatchResult::pass(),
    }
}
