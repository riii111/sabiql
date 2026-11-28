pub(in crate::update) mod connection_list;
pub(in crate::update) mod explorer;
pub(in crate::update) mod focus;
pub(in crate::update) mod input;
pub(in crate::update) mod inspector;

use crate::model::app_state::AppState;

pub(super) fn inspector_max_scroll(state: &AppState) -> usize {
    state
        .inspector_view_model()
        .max_scroll(state.ui.inspector_pane_height)
}

pub(super) fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}
