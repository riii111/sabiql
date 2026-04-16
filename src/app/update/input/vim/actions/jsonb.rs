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
            VimCommand::ModeTransition(VimModeTransition::Insert) => Some(Action::JsonbEnterEdit),
            VimCommand::ModeTransition(VimModeTransition::Append) => {
                Some(Action::JsonbAppendInsert)
            }
            VimCommand::SearchContinuation(SearchContinuation::Next) => {
                Some(Action::JsonbSearchNext)
            }
            VimCommand::SearchContinuation(SearchContinuation::Prev) => {
                Some(Action::JsonbSearchPrev)
            }
            VimCommand::Operator(VimOperator::Yank) => Some(Action::JsonbYankAll),
            VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter)
            | VimCommand::Operator(VimOperator::Delete) => None,
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
        target: InputTarget::JsonbEdit,
        direction,
    })
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

    #[test]
    fn enter_is_ignored_in_viewing_mode() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Enter), ctx);

        assert!(action.is_none());
    }

    #[test]
    fn append_opens_edit_mode() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('A')), ctx);

        assert!(matches!(action, Some(Action::JsonbAppendInsert)));
    }

    #[rstest]
    #[case(Key::Char('0'), CursorMove::LineStart)]
    #[case(Key::Char('$'), CursorMove::LineEnd)]
    #[case(Key::Char('w'), CursorMove::WordForward)]
    #[case(Key::Char('b'), CursorMove::WordBackward)]
    #[case(Key::Char('G'), CursorMove::LastLine)]
    #[case(Key::Char('H'), CursorMove::ViewportTop)]
    #[case(Key::Char('M'), CursorMove::ViewportMiddle)]
    #[case(Key::Char('L'), CursorMove::ViewportBottom)]
    fn extended_navigation_maps_to_cursor_moves(#[case] key: Key, #[case] expected: CursorMove) {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(key), ctx);

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction,
            }) if direction == expected
        ));
    }

    #[test]
    fn gg_moves_to_first_line() {
        let action = crate::app::update::input::vim::action_for_input(
            &combo(Key::Char('g')),
            Some(Prefix::G),
            VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing),
        );

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction: CursorMove::FirstLine,
            })
        ));
    }

    #[test]
    fn search_next_moves_to_match() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('n')), ctx);

        assert!(matches!(action, Some(Action::JsonbSearchNext)));
    }

    #[test]
    fn yank_copies_full_json() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('y')), ctx);

        assert!(matches!(action, Some(Action::JsonbYankAll)));
    }

    #[test]
    fn left_navigation_moves_text_cursor_left() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('h')), ctx);

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction: CursorMove::Left,
            })
        ));
    }
}
