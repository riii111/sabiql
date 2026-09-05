use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::check_er_completion;

pub(super) fn expand_prefetch_with_fk_neighbors(state: &AppState, run_id: u64) -> Vec<Effect> {
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return vec![];
    }
    let seed_tables = state.er_preparation.seed_tables().to_vec();
    vec![Effect::ExtractFkNeighbors {
        run_id,
        seed_tables,
    }]
}

pub(super) fn reduce_er_neighbors(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::FkNeighborsDiscovered { run_id, tables } => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if !state.table_prefetch.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            state.er_preparation.mark_fk_expanded();

            if tables.is_empty() {
                // No new neighbors — proceed to generate with what we have
                return DispatchResult::handled_with(check_er_completion(state));
            }

            for qualified_name in tables {
                state
                    .table_prefetch
                    .queue_pending_table(qualified_name.clone());
            }
            DispatchResult::handled_with(vec![Effect::SchedulePrefetchQueueProcessing {
                run_id: *run_id,
            }])
        }
        _ => DispatchResult::pass(),
    }
}
