use crate::update::action::{
    Action, InputTarget, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
};

use super::{scroll, text_cursor};
use crate::update::input::vim::types::{
    SqlModalVimContext, VimCommand, VimModeTransition, VimNavigation, VimOperator,
};

pub(in crate::update::input::vim) fn command(
    command: VimCommand,
    ctx: SqlModalVimContext,
) -> Option<Action> {
    match ctx {
        SqlModalVimContext::QueryNormal => match command {
            VimCommand::Navigation(navigation) => text_cursor(navigation, InputTarget::SqlModal),
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
