mod error;
mod helpers;
mod lifecycle;
mod selector;
mod setup;

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

/// Handles connection lifecycle, setup form, and error handling.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_connection(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> Option<Vec<Effect>> {
    lifecycle::reduce(state, action, now)
        .or_else(|| setup::reduce(state, action, now))
        .or_else(|| error::reduce(state, action, now))
        .or_else(|| selector::reduce(state, action, now))
}
