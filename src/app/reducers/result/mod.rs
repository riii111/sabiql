mod scroll;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

pub fn reduce_result(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    scroll::reduce(state, action)
}
