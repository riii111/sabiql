use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
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
            plan_text,
            is_analyze,
            execution_time_ms,
        } => {
            let query = state.sql_modal.editor.content().to_string();
            finish_explain_success(
                state,
                plan_text.clone(),
                *is_analyze,
                *execution_time_ms,
                &query,
            );
            DispatchResult::handled()
        }

        Action::ExplainFailed(error) => {
            finish_explain_error(state, error.user_message());
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
