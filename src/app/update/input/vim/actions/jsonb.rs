use crate::app::update::action::{Action, CursorMove, InputTarget};

use crate::app::update::input::vim::types::{
    JsonbDetailVimContext, SearchContinuation, VimCommand, VimModeTransition, VimNavigation,
    VimOperator,
};

pub(in crate::app::update::input::vim) fn command(
    command: VimCommand,
    ctx: JsonbDetailVimContext,
) -> Option<Action> {
    match ctx {
        JsonbDetailVimContext::Viewing => match command {
            VimCommand::Navigation(navigation) => navigation_action(navigation),
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseJsonbDetail),
            VimCommand::ModeTransition(
                VimModeTransition::Insert | VimModeTransition::ConfirmOrEnter,
            ) => Some(Action::JsonbEnterEdit),
            VimCommand::SearchContinuation(SearchContinuation::Next) => {
                Some(Action::JsonbSearchNext)
            }
            VimCommand::SearchContinuation(SearchContinuation::Prev) => {
                Some(Action::JsonbSearchPrev)
            }
            VimCommand::Operator(VimOperator::Yank) => Some(Action::JsonbYankAll),
            VimCommand::Operator(VimOperator::Delete) => None,
        },
        JsonbDetailVimContext::Editing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::JsonbExitEdit),
            _ => None,
        },
        JsonbDetailVimContext::Searching => None,
    }
}

fn navigation_action(navigation: VimNavigation) -> Option<Action> {
    let direction = match navigation {
        VimNavigation::MoveLeft => CursorMove::Left,
        VimNavigation::MoveRight => CursorMove::Right,
        VimNavigation::MoveUp => CursorMove::Up,
        VimNavigation::MoveDown => CursorMove::Down,
        VimNavigation::MoveToFirst => CursorMove::Home,
        VimNavigation::MoveToLast => CursorMove::End,
        _ => return None,
    };

    Some(Action::TextMoveCursor {
        target: InputTarget::JsonbEdit,
        direction,
    })
}
