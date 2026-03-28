use crate::app::update::action::Action;
use crate::app::update::input::keybindings::{JSONB_DETAIL_KEYS, Key, KeyCombo};
use crate::app::update::input::keymap;

pub fn handle_jsonb_detail_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keymap::resolve(&combo, JSONB_DETAIL_KEYS) {
        return action;
    }

    match combo.key {
        Key::Char('j') | Key::Down => Action::JsonbCursorDown,
        Key::Char('k') | Key::Up => Action::JsonbCursorUp,
        Key::Char('h' | 'l') | Key::Left | Key::Right => Action::JsonbToggleFold,
        Key::Char('g') => Action::JsonbScrollToTop,
        Key::Char('G') => Action::JsonbScrollToEnd,
        _ => Action::None,
    }
}
