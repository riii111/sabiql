use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, SmartErRefreshError};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_smart_refresh_failed(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::SmartErRefreshFailed(SmartErRefreshError {
            dsn,
            run_id,
            error,
            new_metadata,
        }) => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.er_preparation.is_current_run(*run_id)
            {
                return DispatchResult::handled();
            }

            if let Some(md) = new_metadata {
                state.session.set_metadata(Some(Arc::clone(md)));
            }

            let Some(metadata) = &state.session.metadata() else {
                state.er_preparation.mark_idle();
                state
                    .messages
                    .set_error_at("Metadata not loaded yet".to_string(), now);
                return DispatchResult::handled();
            };
            let total_table_count = metadata.table_summaries.len();
            let is_scoped = !state.er_preparation.target_tables.is_empty()
                && state.er_preparation.target_tables.len() < total_table_count;

            state.er_preparation.total_tables = total_table_count;
            state.er_preparation.last_signatures.clear();

            if is_scoped {
                state.messages.set_error_at(
                    format!("Smart refresh failed ({error}), falling back to scoped prefetch"),
                    now,
                );
                let scoped_tables = state.er_preparation.target_tables.clone();
                DispatchResult::handled_with(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                        tables: scoped_tables,
                    }]),
                ])
            } else {
                state.messages.set_error_at(
                    format!("Smart refresh failed ({error}), falling back to full refresh"),
                    now,
                );
                DispatchResult::handled_with(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchAll]),
                ])
            }
        }
        _ => DispatchResult::pass(),
    }
}
