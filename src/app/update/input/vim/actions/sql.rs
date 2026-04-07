use crate::app::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

use super::scroll;
use crate::app::update::input::vim::types::{
    SqlModalVimContext, VimCommand, VimModeTransition, VimNavigation, VimOperator,
};

pub(in crate::app::update::input::vim) fn command(
    command: VimCommand,
    ctx: SqlModalVimContext,
) -> Option<Action> {
    match ctx {
        SqlModalVimContext::QueryNormal => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
            VimCommand::ModeTransition(
                VimModeTransition::Insert | VimModeTransition::ConfirmOrEnter,
            ) => Some(Action::SqlModalEnterInsert),
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
        VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
        VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
        _ => None,
    }
}
