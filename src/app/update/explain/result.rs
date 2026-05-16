use std::time::Instant;

use crate::model::app_state::AppState;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::{finish_explain_error, finish_explain_success};

pub(super) fn reduce_result(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::ExplainCompleted {
            dsn,
            run_id,
            query,
            plan_text,
            is_analyze,
            execution_time_ms,
        } => {
            if state.session.dsn.as_ref() != Some(dsn) || !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            finish_explain_success(
                state,
                plan_text.clone(),
                *is_analyze,
                *execution_time_ms,
                query,
            );
            DispatchResult::handled()
        }

        Action::ExplainFailed { dsn, run_id, error } => {
            if state.session.dsn.as_ref() != Some(dsn) || !state.query.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            finish_explain_error(state, error.user_message());
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
