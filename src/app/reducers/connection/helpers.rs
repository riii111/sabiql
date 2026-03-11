use crate::app::connection_cache::ConnectionCache;
use crate::app::state::AppState;

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    ConnectionCache {
        metadata: state.cache.metadata.clone(),
        table_detail: state.cache.table_detail.clone(),
        current_table: state.cache.current_table.clone(),
        query_result: state.query.current_result.clone(),
        result_history: state.query.result_history.clone(),
        explorer_selected: state.ui.explorer_selected,
        inspector_tab: state.ui.inspector_tab,
    }
}

pub(super) fn restore_cache(state: &mut AppState, cache: &ConnectionCache) {
    state.cache.metadata = cache.metadata.clone();
    state.cache.table_detail = cache.table_detail.clone();
    state.cache.current_table = cache.current_table.clone();
    state.query.current_result = cache.query_result.clone();
    state.query.result_history = cache.result_history.clone();
    state.query.history_index = None;
    state.ui.explorer_selected = cache.explorer_selected;
    state.ui.inspector_tab = cache.inspector_tab;
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.ui.result_selection.reset();
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.cell_edit.clear();
    state.ui.staged_delete_rows.clear();
    state.pending_write_preview = None;
}

pub(super) fn reset_connection_state(state: &mut AppState) {
    state.cache.metadata = None;
    state.cache.table_detail = None;
    state.cache.current_table = None;
    state.query.current_result = None;
    state.query.result_history = Default::default();
    state.query.history_index = None;
    state.query.pagination.reset();
    state.ui.set_explorer_selection(None);
    state.ui.result_selection.reset();
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.cell_edit.clear();
    state.ui.staged_delete_rows.clear();
    state.pending_write_preview = None;
}
