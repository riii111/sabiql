mod connection_list;
mod explorer;
mod focus;
mod input;
mod inspector;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn inspector_max_scroll(state: &AppState, services: &AppServices) -> usize {
    state
        .inspector_view_model(services.ddl_generator.as_ref())
        .max_scroll(state.ui.inspector_pane_height())
}

pub(super) fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}

pub fn dispatch_navigation(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> DispatchResult {
    focus::reduce_focus(state, action)
        .or_else(|| input::reduce_input(state, action))
        .or_else(|| explorer::reduce_explorer(state, action))
        .or_else(|| inspector::reduce_inspector(state, action, services))
        .or_else(|| connection_list::reduce_connection_list(state, action, now))
}
