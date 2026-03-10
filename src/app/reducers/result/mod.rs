mod edit;
mod scroll;
mod selection;

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

pub fn reduce_result(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    scroll::reduce(state, action)
        .or_else(|| selection::reduce(state, action))
        .or_else(|| edit::reduce(state, action, now))
}
