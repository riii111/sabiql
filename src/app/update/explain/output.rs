use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::ports::outbound::DbOperationError;

use super::helpers::{finish_explain_error, finish_explain_success};

pub(in crate::update) fn explain_completed(
    state: &mut AppState,
    database_type: DatabaseType,
    database_generation: u64,
    run_id: u64,
    query: String,
    plan_text: String,
    is_analyze: bool,
    execution_time_ms: u64,
) -> Vec<Effect> {
    if state.is_stale_explain_run(database_generation, run_id) {
        return vec![];
    }
    finish_explain_success(
        state,
        plan_text,
        database_type,
        is_analyze,
        execution_time_ms,
        &query,
    );
    vec![]
}

pub(in crate::update) fn explain_failed(
    state: &mut AppState,
    database_generation: u64,
    run_id: u64,
    error: DbOperationError,
) -> Vec<Effect> {
    if state.is_stale_explain_run(database_generation, run_id) {
        return vec![];
    }
    finish_explain_error(state, error.user_message());
    vec![]
}
