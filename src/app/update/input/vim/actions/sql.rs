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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use crate::app::update::input::vim::{VimSurfaceContext, action_for_key};
    use rstest::rstest;

    fn combo(key: Key) -> KeyCombo {
        KeyCombo::plain(key)
    }

    fn combo_ctrl(key: Key) -> KeyCombo {
        KeyCombo::ctrl(key)
    }

    #[rstest]
    #[case(Key::Char('i'))]
    #[case(Key::Enter)]
    fn insert_and_confirm_enter_insert(#[case] key: Key) {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(key), ctx);

        assert!(matches!(action, Some(Action::SqlModalEnterInsert)));
    }

    #[test]
    fn yank_copies_query() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(Key::Char('y')), ctx);

        assert!(matches!(action, Some(Action::SqlModalYank)));
    }

    #[rstest]
    #[case(Key::Char('n'), ScrollDirection::Down)]
    #[case(Key::Char('p'), ScrollDirection::Up)]
    fn ctrl_aliases_scroll_by_line(#[case] key: Key, #[case] expected_direction: ScrollDirection) {
        let action = action_for_key(
            &combo_ctrl(key),
            VimSurfaceContext::SqlModal(SqlModalVimContext::PlanViewer),
        );

        assert!(matches!(
            action,
            Some(Action::Scroll {
                target: ScrollTarget::ExplainPlan,
                direction,
                amount: ScrollAmount::Line,
            }) if direction == expected_direction
        ));
    }
}
