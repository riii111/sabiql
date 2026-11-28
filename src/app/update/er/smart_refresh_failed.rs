use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::SmartErRefreshError;
use crate::update::browse::metadata::prefetch::start_er_prefetch_all;

pub(super) fn smart_er_refresh_failed(
    state: &mut AppState,
    SmartErRefreshError {
        error,
        new_metadata,
    }: SmartErRefreshError,
) -> Vec<Effect> {
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
    effects.extend(start_er_prefetch_all(state));
    effects
}
