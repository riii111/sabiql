use crate::app::model::app_state::AppState;
use crate::app::model::connection::cache::ConnectionCache;

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected,
        state.ui.inspector_tab,
        state.query.current_result().cloned(),
        state.query.result_history().clone(),
    )
}

pub(super) fn restore_cache(state: &mut AppState, cache: &ConnectionCache) {
    state.session.restore_from_cache(cache, &mut state.query);
    state.ui.explorer_selected = cache.explorer_selected;
    state.ui.inspector_tab = cache.inspector_tab;
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.result_interaction.reset_view();
}
