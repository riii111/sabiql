use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, SmartErRefreshError};
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(in crate::update) fn smart_er_refresh_failed(
    state: &mut AppState,
    failure: SmartErRefreshError,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let SmartErRefreshError {
        endpoint,
        run_id,
        error,
        new_metadata,
    } = failure;
    if !state.session.endpoint_matches(&endpoint) || !state.er_preparation.is_current_run(run_id) {
        return vec![];
    }

    let mut effects = Vec::new();

    if let Some(md) = new_metadata {
        state.session.set_metadata(Some(md));
    }

    let Some(metadata) = &state.session.metadata() else {
        state.er_preparation.mark_idle();
        state
            .messages
            .set_error("Metadata not loaded yet".to_string());
        return effects;
    };
    state
        .er_preparation
        .invalidate_refresh_signatures(metadata.table_summaries.len());

    state.messages.set_error(format!(
        "Smart refresh failed ({error}), falling back to full refresh"
    ));
    state.table_detail_cache.clear();
    effects.push(Effect::DispatchActions(vec![Action::StartErPrefetchAll]));
    effects
}
