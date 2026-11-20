mod cell_detail;
mod common;
mod confirm;
mod connection_error;
mod connection_selector;
mod context;
mod help;
mod legacy;
mod model;
mod pickers;
mod row_detail;
mod scroll;
mod settings;
mod sqlite_diagnostics;

use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, InputTarget};

pub use cell_detail::{cell_detail_action, cell_detail_hints};
pub use confirm::{confirm_action, confirm_hints};
pub use connection_error::{connection_error_action, connection_error_hints};
pub use connection_selector::{connection_selector_action, connection_selector_hints};
pub use context::KeyContext;
pub use help::{help_action, help_hints};
pub use legacy::{resolve, resolve_mode, resolve_mode_with_policy, resolve_with_policy};
pub use pickers::{Picker, picker_action, picker_hints};
pub use row_detail::{row_detail_action, row_detail_hints};
pub use settings::{settings_action, settings_hints};
pub use sqlite_diagnostics::{sqlite_diagnostics_action, sqlite_diagnostics_hints};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSurface {
    Footer,
    Border,
    Current,
    SearchFilter,
    Advanced,
    ModalBasics,
    Connections,
    Details,
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
