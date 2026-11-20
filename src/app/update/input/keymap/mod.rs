mod common;
mod legacy;
mod model;
mod table_picker;

use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, InputTarget};

pub use legacy::{resolve, resolve_mode, resolve_mode_with_policy, resolve_with_policy};
pub use table_picker::{table_picker_action, table_picker_hints};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSurface {
    Footer,
    Border,
    Current,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub short: &'static str,
    pub description: &'static str,
}

pub(super) fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    common::readline_action_for(combo, target)
}
