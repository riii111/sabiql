use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;

pub use crate::update::input::keybindings::{Key, KeyCombo as InputKeyCombo, Modifiers};

#[derive(Clone, Debug)]
pub enum InputEvent {
    Init,
    Key(InputKeyCombo),
    Paste(String),
    Resize(u16, u16),
}

pub fn handle_input(event: InputEvent, state: &AppState, services: &AppServices) -> Action {
    crate::update::input::dispatch::resolve_input(event, state, services)
}
