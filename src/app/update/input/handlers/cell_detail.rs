use crate::model::browse::cell_detail::CellDetailMode;
use crate::model::shared::key_sequence::Prefix;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
use crate::update::input::keybindings::{
    CELL_DETAIL, CELL_DETAIL_EDIT, CELL_DETAIL_SEARCH_KEYS, Key, KeyCombo, Modifiers,
};
use crate::update::input::keymap;
use crate::update::input::vim::{
    SearchContinuation, VimCommand, VimModeTransition, VimNavigation, VimOperator,
    classify_command, classify_sequence,
};

pub fn handle_cell_detail_keys(
    combo: KeyCombo,
    mode: CellDetailMode,
    pending_prefix: Option<Prefix>,
) -> Action {
    match mode {
        CellDetailMode::Searching => handle_search_input(combo),
        CellDetailMode::Editing => handle_edit_input(combo),
        CellDetailMode::Viewing => handle_viewing_input(combo, pending_prefix),
    }
}

fn handle_viewing_input(combo: KeyCombo, pending_prefix: Option<Prefix>) -> Action {
    if let Some(prefix) = pending_prefix {
        if combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
            return Action::CancelKeySequence;
        }
        return match classify_sequence(prefix, &combo).and_then(cell_detail_viewing_action) {
            Some(Action::None) | None => Action::CancelKeySequence,
            Some(action) => action,
        };
    }

    if !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) && combo.key == Key::Char('g')
    {
        return Action::BeginKeySequence(Prefix::G);
    }

    if !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
        match combo.key {
            Key::Home => {
                return Action::TextMoveCursor {
                    target: InputTarget::CellDetailEdit,
                    direction: CursorMove::LineStart,
                };
            }
            Key::End => {
                return Action::TextMoveCursor {
                    target: InputTarget::CellDetailEdit,
                    direction: CursorMove::LineEnd,
                };
            }
            _ => {}
        }
    }

    if let Some(action) = classify_command(&combo).and_then(cell_detail_viewing_action) {
        return action;
    }

    CELL_DETAIL.resolve(&combo).unwrap_or(Action::None)
}

fn handle_edit_input(combo: KeyCombo) -> Action {
    if let Some(action) = classify_command(&combo).and_then(cell_detail_editing_action) {
        return action;
    }

    if let Some(action) = CELL_DETAIL_EDIT.resolve(&combo) {
        return action;
    }

    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::CellDetailEdit,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::CellDetailEdit,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::CellDetailEdit,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::Right,
        },
        Key::Up => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::Up,
        },
        Key::Down => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::Down,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::CellDetailEdit,
            direction: CursorMove::End,
        },
        Key::Enter => Action::TextInput {
            target: InputTarget::CellDetailEdit,
            ch: '\n',
        },
        Key::Tab => Action::TextInput {
            target: InputTarget::CellDetailEdit,
            ch: '\t',
        },
        _ => Action::None,
    }
}

fn cell_detail_viewing_action(command: VimCommand) -> Option<Action> {
    match command {
        VimCommand::Navigation(navigation) => navigation_action(navigation),
        VimCommand::ModeTransition(VimModeTransition::Escape) => {
            Some(Action::CloseModal(ModalKind::CellDetail))
        }
        VimCommand::ModeTransition(VimModeTransition::Insert) => Some(Action::CellDetailEnterEdit),
        VimCommand::ModeTransition(VimModeTransition::Append) => {
            Some(Action::CellDetailAppendInsert)
        }
        VimCommand::SearchContinuation(SearchContinuation::Next) => {
            Some(Action::CellDetailSearchNext)
        }
        VimCommand::SearchContinuation(SearchContinuation::Prev) => {
            Some(Action::CellDetailSearchPrev)
        }
        VimCommand::Operator(VimOperator::Yank) => Some(Action::CellDetailYankAll),
        VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter)
        | VimCommand::Operator(VimOperator::Delete) => None,
    }
}

fn cell_detail_editing_action(command: VimCommand) -> Option<Action> {
    match command {
        VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CellDetailExitEdit),
        _ => None,
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
        target: InputTarget::CellDetailEdit,
        direction,
    })
}

fn handle_search_input(combo: KeyCombo) -> Action {
    if let Some(action) = keymap::resolve(&combo, CELL_DETAIL_SEARCH_KEYS) {
        return action;
    }

    if combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
        return Action::None;
    }

    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::CellDetailSearch,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::CellDetailSearch,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::CellDetailSearch,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::CellDetailSearch,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::CellDetailSearch,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::CellDetailSearch,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::CellDetailSearch,
            direction: CursorMove::End,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn modified_combo(k: Key, modifiers: Modifiers) -> KeyCombo {
        KeyCombo { key: k, modifiers }
    }

    #[test]
    fn enter_confirms_active_search() {
        let result = handle_cell_detail_keys(combo(Key::Enter), CellDetailMode::Searching, None);

        assert!(matches!(result, Action::CellDetailSearchSubmit));
    }

    #[test]
    fn ctrl_char_is_not_inserted_into_search() {
        let result = handle_cell_detail_keys(
            modified_combo(Key::Char('n'), Modifiers::CTRL),
            CellDetailMode::Searching,
            None,
        );

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn alt_char_is_not_inserted_into_search() {
        let result = handle_cell_detail_keys(
            modified_combo(Key::Char('n'), Modifiers::ALT),
            CellDetailMode::Searching,
            None,
        );

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn j_moves_down_when_viewing() {
        let result = handle_cell_detail_keys(combo(Key::Char('j')), CellDetailMode::Viewing, None);

        assert!(matches!(
            result,
            Action::TextMoveCursor {
                target: InputTarget::CellDetailEdit,
                direction: CursorMove::Down,
            }
        ));
    }

    #[test]
    fn slash_enters_search_when_viewing() {
        let result = handle_cell_detail_keys(combo(Key::Char('/')), CellDetailMode::Viewing, None);

        assert!(matches!(result, Action::CellDetailEnterSearch));
    }

    #[test]
    fn i_enters_cell_detail_edit_when_viewing() {
        let result = handle_cell_detail_keys(combo(Key::Char('i')), CellDetailMode::Viewing, None);

        assert!(matches!(result, Action::CellDetailEnterEdit));
    }

    #[test]
    fn esc_exits_editing_to_viewing() {
        let result = handle_cell_detail_keys(combo(Key::Esc), CellDetailMode::Editing, None);

        assert!(matches!(result, Action::CellDetailExitEdit));
    }

    #[test]
    fn enter_in_editing_inserts_newline() {
        let result = handle_cell_detail_keys(combo(Key::Enter), CellDetailMode::Editing, None);

        assert!(matches!(
            result,
            Action::TextInput {
                target: InputTarget::CellDetailEdit,
                ch: '\n'
            }
        ));
    }
}
