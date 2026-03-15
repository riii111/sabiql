use crate::app::connection_cache::ConnectionCache;
use crate::app::state::AppState;

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected,
        state.ui.inspector_tab,
        state.query.current_result.clone(),
        state.query.result_history.clone(),
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

pub(super) fn reset_connection_state(state: &mut AppState) {
    state.session.set_metadata(None);
    state.session.set_table_detail_raw(None);
    state.session.set_current_table(None);
    state.query.current_result = None;
    state.query.result_history = Default::default();
    state.query.history_index = None;
    state.query.pagination.reset();
    state.ui.set_explorer_selection(None);
    state.result_interaction.reset_view();
}
