pub(in crate::update) mod connection_list;
pub(in crate::update) mod explorer;
pub(in crate::update) mod focus;
pub(in crate::update) mod input;
pub(in crate::update) mod inspector;

use crate::model::app_state::AppState;
use crate::services::AppServices;

pub(super) fn inspector_max_scroll(state: &AppState, services: &AppServices) -> usize {
    state
        .inspector_view_model(services.ddl_generator.as_ref())
        .max_scroll(state.ui.inspector_pane_height)
}

pub(super) fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}
