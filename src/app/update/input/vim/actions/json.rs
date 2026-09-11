use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};

use crate::update::input::vim::types::{
    JsonDetailVimContext, SearchContinuation, VimCommand, VimModeTransition, VimNavigation,
    VimOperator,
};

pub(in crate::update::input::vim) fn command(
    command: VimCommand,
    ctx: JsonDetailVimContext,
) -> Option<Action> {
    match ctx {
        JsonDetailVimContext::Viewing => match command {
            VimCommand::Navigation(navigation) => navigation_action(navigation),
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

fn navigation_action(navigation: VimNavigation) -> Option<Action> {
    let direction = match navigation {
        VimNavigation::MoveLeft => CursorMove::Left,
        VimNavigation::MoveRight => CursorMove::Right,
        VimNavigation::MoveUp => CursorMove::Up,
        VimNavigation::MoveDown => CursorMove::Down,
        VimNavigation::MoveToFirst => CursorMove::FirstLine,
        VimNavigation::MoveToLast => CursorMove::LastLine,
        VimNavigation::MoveLineStart => CursorMove::LineStart,
        VimNavigation::MoveLineEnd => CursorMove::LineEnd,
        VimNavigation::MoveWordForward => CursorMove::WordForward,
        VimNavigation::MoveWordBackward => CursorMove::WordBackward,
        VimNavigation::ViewportTop => CursorMove::ViewportTop,
        VimNavigation::ViewportMiddle => CursorMove::ViewportMiddle,
        VimNavigation::ViewportBottom => CursorMove::ViewportBottom,
        _ => return None,
    };

    Some(Action::TextMoveCursor {
        target: InputTarget::JsonEdit,
        direction,
    })
}
