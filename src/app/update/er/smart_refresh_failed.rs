use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::ports::outbound::DbOperationError;
use crate::update::action::{Action, SmartErRefreshError};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_smart_refresh_failed(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::SmartErRefreshFailed(SmartErRefreshError {
            dsn,
            run_id,
            error,
            new_metadata,
        }) => {
            if !state.session.dsn_matches(dsn) || !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            let (DbOperationError::ObjectMissing(_), Some(metadata)) = (error, new_metadata) else {
                state.er_preparation.invalidate_run();
                state.messages.set_error(format!(
                    "ER refresh failed: {} 'e' to retry after recovery.",
                    error.user_message()
                ));
                return DispatchResult::handled();
            };

            state.session.set_metadata(Some(Arc::clone(metadata)));
            state
                .er_preparation
                .invalidate_refresh_signatures(metadata.table_summaries.len());

            state.messages.set_error(format!(
                "Smart refresh failed ({error}), falling back to full refresh"
            ));
            DispatchResult::handled_with(vec![
                Effect::ClearCompletionEngineCache,
                Effect::DispatchActions(vec![Action::StartErPrefetchAll]),
            ])
        }
        _ => DispatchResult::pass(),
    }
}
