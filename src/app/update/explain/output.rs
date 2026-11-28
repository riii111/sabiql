use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::ports::outbound::DbOperationError;
use crate::update::action::ExplainPlan;

use super::helpers::{finish_explain_error, finish_explain_success};

pub(in crate::update) fn explain_completed(
    state: &mut AppState,
    database_generation: u64,
    run_id: u64,
    is_analyze: bool,
    outcome: Result<ExplainPlan, DbOperationError>,
) -> Vec<Effect> {
    if state.is_stale_explain_run(database_generation, run_id) {
        return vec![];
    }
    match outcome {
        Ok(ExplainPlan {
            database_type,
            query,
            plan_text,
            execution_time_ms,
        }) => finish_explain_success(
            state,
            plan_text,
            database_type,
            is_analyze,
            execution_time_ms,
            &query,
        ),
        Err(error) => finish_explain_error(state, error.user_message()),
    }
    vec![]
}
