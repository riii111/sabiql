mod actions;
mod classify;
mod dispatch;
mod types;

use crate::app::model::shared::key_sequence::Prefix;
use crate::app::update::action::Action;
use crate::app::update::input::keybindings::KeyCombo;

pub use classify::{classify_command, classify_sequence};
pub use types::{
    BrowseVimContext, InspectorVimContext, JsonbDetailVimContext, ResultVimContext,
    SearchContinuation, SqlModalVimContext, VimCommand, VimModeTransition, VimNavigation,
    VimOperator, VimSurfaceContext,
};

pub fn action_for_input(
    combo: &KeyCombo,
    pending_prefix: Option<Prefix>,
    ctx: VimSurfaceContext,
) -> Option<Action> {
    let command = if let Some(prefix) = pending_prefix {
        classify_sequence(prefix, combo)?
    } else {
        classify_command(combo)?
    };

    action_for_command(command, ctx)
}

pub fn action_for_key(combo: &KeyCombo, ctx: VimSurfaceContext) -> Option<Action> {
    action_for_input(combo, None, ctx)
}

pub fn action_for_command(command: VimCommand, ctx: VimSurfaceContext) -> Option<Action> {
    dispatch::surface(command, ctx)
}
