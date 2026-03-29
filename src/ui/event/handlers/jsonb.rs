use crate::app::update::action::{Action, CursorMove, InputTarget};
use crate::app::update::input::keybindings::{
    JSONB_DETAIL_KEYS, JSONB_EDIT_KEYS, JSONB_SEARCH_KEYS, Key, KeyCombo,
};
use crate::app::update::input::keymap;

pub fn handle_jsonb_detail_keys(combo: KeyCombo, is_searching: bool) -> Action {
    if is_searching {
        return handle_search_input(combo);
    }

    keymap::resolve(&combo, JSONB_DETAIL_KEYS).unwrap_or(Action::None)
}

fn handle_search_input(combo: KeyCombo) -> Action {
    // Command keys (Enter/Esc) resolved from SSOT keybindings
    if let Some(action) = keymap::resolve(&combo, JSONB_SEARCH_KEYS) {
        return action;
    }

    // Text input fallthrough
    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::JsonbSearch,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::JsonbSearch,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::End,
        },
        _ => Action::None,
    }
}

pub fn handle_jsonb_edit_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keymap::resolve(&combo, JSONB_EDIT_KEYS) {
        return action;
    }

    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::JsonbEdit,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::JsonbEdit,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Right,
        },
        Key::Up => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Up,
        },
        Key::Down => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Down,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::End,
        },
        Key::Enter => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: '\n',
        },
        Key::Tab => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: '\t',
        },
        _ => Action::None,
    }
}
