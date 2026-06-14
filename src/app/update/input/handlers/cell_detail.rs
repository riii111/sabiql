use crate::update::action::{Action, CursorMove, InputTarget};
use crate::update::input::keybindings::{
    CELL_DETAIL, CELL_DETAIL_SEARCH_KEYS, Key, KeyCombo, Modifiers,
};
use crate::update::input::keymap;

pub fn handle_cell_detail_keys(combo: KeyCombo, is_searching: bool) -> Action {
    if is_searching {
        return handle_search_input(combo);
    }

    CELL_DETAIL.resolve(&combo).unwrap_or(Action::None)
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
    use crate::update::action::{ScrollAmount, ScrollDirection, ScrollTarget};

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn modified_combo(k: Key, modifiers: Modifiers) -> KeyCombo {
        KeyCombo { key: k, modifiers }
    }

    #[test]
    fn enter_confirms_active_search() {
        let result = handle_cell_detail_keys(combo(Key::Enter), true);

        assert!(matches!(result, Action::CellDetailSearchSubmit));
    }

    #[test]
    fn ctrl_char_is_not_inserted_into_search() {
        let result = handle_cell_detail_keys(modified_combo(Key::Char('n'), Modifiers::CTRL), true);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn alt_char_is_not_inserted_into_search() {
        let result = handle_cell_detail_keys(modified_combo(Key::Char('n'), Modifiers::ALT), true);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn j_scrolls_down_when_not_searching() {
        let result = handle_cell_detail_keys(combo(Key::Char('j')), false);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::CellDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn slash_enters_search_when_not_searching() {
        let result = handle_cell_detail_keys(combo(Key::Char('/')), false);

        assert!(matches!(result, Action::CellDetailEnterSearch));
    }
}
