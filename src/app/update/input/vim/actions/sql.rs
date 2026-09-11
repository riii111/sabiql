use crate::update::action::{
    Action, CursorMove, InputTarget, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
};

use super::scroll;
use crate::update::input::vim::types::{
    SqlModalVimContext, VimCommand, VimModeTransition, VimNavigation, VimOperator,
};

pub(in crate::update::input::vim) fn command(
    command: VimCommand,
    ctx: SqlModalVimContext,
) -> Option<Action> {
    match ctx {
        SqlModalVimContext::QueryNormal => match command {
            VimCommand::Navigation(navigation) => query_navigation(navigation),
            VimCommand::ModeTransition(VimModeTransition::Escape) => {
                Some(Action::CloseModal(ModalKind::SqlModal))
            }
            VimCommand::ModeTransition(VimModeTransition::Append) => {
                Some(Action::SqlModalAppendInsert)
            }
            VimCommand::ModeTransition(VimModeTransition::Insert) => {
                Some(Action::SqlModalEnterInsert)
            }
            VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
            _ => None,
        },
        SqlModalVimContext::QueryEditing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => {
                Some(Action::SqlModalEnterNormal)
            }
            _ => None,
        },
        SqlModalVimContext::PlanViewer => viewer(command, ScrollTarget::ExplainPlan),
        SqlModalVimContext::CompareViewer => viewer(command, ScrollTarget::ExplainCompare),
    }
}

fn query_navigation(navigation: VimNavigation) -> Option<Action> {
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
        target: InputTarget::SqlModal,
        direction,
    })
}

fn viewer(command: VimCommand, target: ScrollTarget) -> Option<Action> {
    match command {
        VimCommand::Navigation(VimNavigation::MoveDown) => {
            Some(scroll(target, ScrollDirection::Down, ScrollAmount::Line))
        }
        VimCommand::Navigation(VimNavigation::MoveUp) => {
            Some(scroll(target, ScrollDirection::Up, ScrollAmount::Line))
        }
        VimCommand::ModeTransition(VimModeTransition::Escape) => {
            Some(Action::CloseModal(ModalKind::SqlModal))
        }
        VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
        _ => None,
    }
}
