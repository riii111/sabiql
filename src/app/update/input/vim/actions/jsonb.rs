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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use crate::app::update::input::vim::{VimSurfaceContext, action_for_key};

    fn combo(key: Key) -> KeyCombo {
        KeyCombo::plain(key)
    }

    #[test]
    fn enter_opens_edit_mode() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Enter), ctx);

        assert!(matches!(action, Some(Action::JsonbEnterEdit)));
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
