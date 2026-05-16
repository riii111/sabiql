mod edit;
mod history;
mod jsonb;
mod scroll;
mod selection;
mod yank;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_result(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> DispatchResult {
    scroll::reduce_scroll(state, action)
        .or_else(|| selection::reduce_selection(state, action, now))
        .or_else(|| edit::reduce_edit(state, action, now))
        .or_else(|| yank::reduce_yank(state, action, services, now))
        .or_else(|| history::reduce_history(state, action))
        .or_else(|| jsonb::reduce_jsonb(state, action, now))
}
