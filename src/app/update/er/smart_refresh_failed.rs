use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, SmartErRefreshError};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_smart_refresh_failed(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
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

            if let Some(md) = new_metadata {
                state.session.set_metadata(Some(Arc::clone(md)));
            }

            let Some(metadata) = &state.session.metadata() else {
                state.er_preparation.mark_idle();
                state.set_error("Metadata not loaded yet".to_string());
                return DispatchResult::handled();
            };
            let scoped_tables = state
                .er_preparation
                .scoped_fallback_tables(metadata.table_summaries.len());
            state
                .er_preparation
                .invalidate_refresh_signatures(metadata.table_summaries.len());

            if let Some(scoped_tables) = scoped_tables {
                state.set_error(format!(
                    "Smart refresh failed ({error}), falling back to scoped prefetch"
                ));
                DispatchResult::handled_with(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchScoped {
                        tables: scoped_tables,
                    }]),
                ])
            } else {
                state.set_error(format!(
                    "Smart refresh failed ({error}), falling back to full refresh"
                ));
                DispatchResult::handled_with(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::DispatchActions(vec![Action::StartPrefetchAll]),
                ])
            }
        }
        _ => DispatchResult::pass(),
    }
}
