mod edit;
mod history;
mod jsonb;
mod scroll;
mod selection;
mod yank;

use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::services::AppServices;
use crate::app::update::action::Action;

pub fn reduce_result(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> Option<Vec<Effect>> {
    scroll::reduce(state, action)
        .or_else(|| selection::reduce(state, action, now))
        .or_else(|| edit::reduce(state, action, now))
        .or_else(|| yank::reduce(state, action, services, now))
        .or_else(|| history::reduce(state, action))
        .or_else(|| jsonb::reduce(state, action, now))
}
