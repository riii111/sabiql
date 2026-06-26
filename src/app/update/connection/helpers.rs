use crate::model::app_state::AppState;
use crate::model::connection::cache::ConnectionCache;
use crate::update::action::ConnectionTarget;

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected(),
        state.ui.inspector_tab(),
        state.query.current_result().cloned(),
        state.query.result_history().clone(),
    )
}

pub(super) fn reset_connection_scoped_er_state(state: &mut AppState) {
    state.ui.reset_er_picker_request();
    state.er_preparation.reset();
}

pub(super) fn reset_active_connection_state(state: &mut AppState) {
    state.session.reset(&mut state.query);
    state.result_interaction.reset_view();
    state.ui.set_explorer_selection(None);
    reset_connection_scoped_er_state(state);
}

pub(super) fn restore_cache(
    state: &mut AppState,
    cache: &ConnectionCache,
    target: &ConnectionTarget,
) {
    state.session.restore_from_cache_for_connection(
        cache,
        &mut state.query,
        &target.id,
        &target.name,
        target.database_type,
        &target.dsn,
    );
    state.ui.set_inspector_tab(
        state
            .session
            .active_db_capabilities()
            .normalize_inspector_tab(cache.inspector_tab),
    );
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.result_interaction.reset_view();
}
