use crate::domain::connection::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::connection::cache::ConnectionCache;
use crate::services::AppServices;

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected(),
        state.ui.inspector_tab(),
        state.query.current_result().cloned(),
        state.query.result_history().clone(),
    )
}

pub(super) fn restore_cache(
    state: &mut AppState,
    cache: &ConnectionCache,
    id: &ConnectionId,
    name: &str,
    database_type: DatabaseType,
    dsn: &str,
    services: &AppServices,
) {
    state.session.restore_from_cache_for_connection(
        cache,
        &mut state.query,
        id,
        name,
        database_type,
        dsn,
    );
    state.ui.set_explorer_selected_raw(cache.explorer_selected);
    state.ui.set_inspector_tab(
        services
            .db_capabilities
            .normalize_inspector_tab(cache.inspector_tab),
    );
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.result_interaction.reset_view();
}
