use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::check_er_completion;

pub(super) fn expand_prefetch_with_fk_neighbors(state: &AppState, run_id: u64) -> Vec<Effect> {
    if !state.sql_modal.is_current_prefetch_run(run_id) {
        return vec![];
    }
    let seed_tables = state.er_preparation.seed_tables().to_vec();
    vec![Effect::ExtractFkNeighbors {
        run_id,
        seed_tables,
    }]
}

pub(super) fn reduce_er_neighbors(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ExpandPrefetchWithFkNeighbors { run_id } => {
            DispatchResult::handled_with(expand_prefetch_with_fk_neighbors(state, *run_id))
        }
        Action::FkNeighborsDiscovered { run_id, tables } => {
            if !state.sql_modal.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_fk_expanded();

            if tables.is_empty() {
                // No new neighbors — proceed to generate with what we have
                return DispatchResult::handled_with(check_er_completion(state, now));
            }

            for qualified_name in tables {
                let is_new_pending = state
                    .er_preparation
                    .queue_pending_table(qualified_name.clone());
                if is_new_pending {
                    state.sql_modal.queue_table_prefetch(qualified_name.clone());
                }
            }
            DispatchResult::handled_with(vec![Effect::ProcessPrefetchQueue { run_id: *run_id }])
        }
        _ => DispatchResult::pass(),
    }
}
