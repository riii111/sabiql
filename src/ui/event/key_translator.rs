pub use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::keybindings::{Key, KeyCombo, Modifiers};

/// Translate a crossterm `KeyEvent` into the app-layer `KeyCombo`.
pub fn translate(event: KeyEvent) -> KeyCombo {
    let key = match event.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        KeyCode::Null => Key::Null,
        _ => Key::Other,
    };

    let modifiers = Modifiers {
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
    };

    KeyCombo { key, modifiers }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_char_translates_to_char_no_modifiers() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::plain(Key::Char('a')));
    }

    #[test]
    fn ctrl_char_translates_to_ctrl_modifier() {
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::ctrl(Key::Char('p')));
    }

    #[test]
    fn alt_enter_translates_to_alt_modifier() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::alt(Key::Enter));
    }

    #[test]
    fn backtab_translates_with_shift() {
        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);

        let combo = translate(event);

        assert_eq!(
            combo,
            KeyCombo {
                key: Key::BackTab,
                modifiers: Modifiers {
                    ctrl: false,
                    alt: false,
                    shift: true,
                },
            }
        );
    }

    #[test]
    fn null_key_translates_to_null() {
        let event = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::plain(Key::Null));
    }

    #[test]
    fn unknown_key_translates_to_other() {
        let event = KeyEvent::new(KeyCode::CapsLock, KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::plain(Key::Other));
    }

    #[test]
    fn arrow_keys_translate_correctly() {
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            KeyCombo::plain(Key::Up)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            KeyCombo::plain(Key::Down)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            KeyCombo::plain(Key::Left)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            KeyCombo::plain(Key::Right)
        );
    }

    #[test]
    fn function_key_translates() {
        let event = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::plain(Key::F(1)));
    }
}
