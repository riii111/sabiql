use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
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

            let mut effects = Vec::new();

            if let Some(md) = new_metadata {
                state.session.set_metadata(Some(Arc::clone(md)));
            }

            let Some(metadata) = &state.session.metadata() else {
                state.er_preparation.mark_idle();
                state
                    .messages
                    .set_error("Metadata not loaded yet".to_string());
                return DispatchResult::handled_with(effects);
            };
            state
                .er_preparation
                .invalidate_refresh_signatures(metadata.table_summaries.len());

            state.messages.set_error(format!(
                "Smart refresh failed ({error}), falling back to full refresh"
            ));
            effects.extend([
                Effect::ClearCompletionEngineCache,
                Effect::DispatchActions(vec![Action::StartErPrefetchAll]),
            ]);
            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}
