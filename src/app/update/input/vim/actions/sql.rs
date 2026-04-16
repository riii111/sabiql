use crate::app::update::action::{
    Action, CursorMove, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
};

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
            VimCommand::Navigation(navigation) => query_navigation(navigation),
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
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
        VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
        VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::key_sequence::Prefix;
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
    #[case(Key::Char('i'), false)]
    #[case(Key::Char('A'), true)]
    fn insert_and_confirm_enter_insert(#[case] key: Key, #[case] append: bool) {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(key), ctx);

        if append {
            assert!(matches!(action, Some(Action::SqlModalAppendInsert)));
        } else {
            assert!(matches!(action, Some(Action::SqlModalEnterInsert)));
        }
    }

    #[test]
    fn yank_copies_query() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(Key::Char('y')), ctx);

        assert!(matches!(action, Some(Action::SqlModalYank)));
    }

    #[rstest]
    #[case(Key::Char('h'), CursorMove::Left)]
    #[case(Key::Char('j'), CursorMove::Down)]
    #[case(Key::Char('k'), CursorMove::Up)]
    #[case(Key::Char('l'), CursorMove::Right)]
    #[case(Key::Char('G'), CursorMove::LastLine)]
    #[case(Key::Char('0'), CursorMove::LineStart)]
    #[case(Key::Char('$'), CursorMove::LineEnd)]
    #[case(Key::Char('w'), CursorMove::WordForward)]
    #[case(Key::Char('b'), CursorMove::WordBackward)]
    #[case(Key::Char('H'), CursorMove::ViewportTop)]
    #[case(Key::Char('M'), CursorMove::ViewportMiddle)]
    #[case(Key::Char('L'), CursorMove::ViewportBottom)]
    fn normal_navigation_moves_sql_cursor(#[case] key: Key, #[case] expected: CursorMove) {
        let action = action_for_key(
            &combo(key),
            VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal),
        );

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction,
            }) if direction == expected
        ));
    }

    #[test]
    fn gg_moves_to_first_line() {
        let action = crate::app::update::input::vim::action_for_input(
            &combo(Key::Char('g')),
            Some(Prefix::G),
            VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal),
        );

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::FirstLine,
            })
        ));
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
