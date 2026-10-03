use crate::update::action::{Action, InputTarget, ModalKind};

use super::text_cursor;
use crate::update::input::vim::types::{
    JsonDetailVimContext, SearchContinuation, VimCommand, VimModeTransition, VimOperator,
};

pub(in crate::update::input::vim) fn command(
    command: VimCommand,
    ctx: JsonDetailVimContext,
) -> Option<Action> {
    match ctx {
        JsonDetailVimContext::Viewing => match command {
            VimCommand::Navigation(navigation) => text_cursor(navigation, InputTarget::JsonEdit),
            VimCommand::ModeTransition(VimModeTransition::Escape) => {
                Some(Action::CloseModal(ModalKind::JsonDetail))
            }
            VimCommand::ModeTransition(VimModeTransition::Insert) => Some(Action::JsonEnterEdit),
            VimCommand::ModeTransition(VimModeTransition::Append) => Some(Action::JsonAppendInsert),
            VimCommand::SearchContinuation(SearchContinuation::Next) => {
                Some(Action::JsonSearchNext)
            }
            VimCommand::SearchContinuation(SearchContinuation::Prev) => {
                Some(Action::JsonSearchPrev)
            }
            VimCommand::Operator(VimOperator::Yank) => Some(Action::JsonYankAll),
            VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter)
            | VimCommand::Operator(VimOperator::Delete) => None,
        },
        JsonDetailVimContext::Editing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::JsonExitEdit),
            _ => None,
        },
        JsonDetailVimContext::Searching => None,
    }
}
