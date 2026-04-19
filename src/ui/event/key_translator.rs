use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::ports::inbound::{InputKeyCombo, Key, Modifiers};

pub fn translate(event: KeyEvent) -> InputKeyCombo {
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

    // Normalize: uppercase Key::Char already encodes shift, so drop the shift
    // flag to prevent double-encoding (e.g. Kitty sends 'G' + SHIFT).
    let raw_shift = event.modifiers.contains(KeyModifiers::SHIFT);
    let shift = match key {
        Key::Char(c) if c.is_ascii_uppercase() => false,
        _ => raw_shift,
    };

    let modifiers = Modifiers {
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift,
    };

    InputKeyCombo { key, modifiers }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_char_translates_to_char_no_modifiers() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::plain(Key::Char('a')));
    }

    #[test]
    fn ctrl_char_translates_to_ctrl_modifier() {
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::ctrl(Key::Char('p')));
    }

    #[test]
    fn alt_enter_translates_to_alt_modifier() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::alt(Key::Enter));
    }

    #[test]
    fn backtab_translates_with_shift() {
        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);

        let combo = translate(event);

        assert_eq!(
            combo,
            InputKeyCombo {
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

        assert_eq!(combo, InputKeyCombo::plain(Key::Null));
    }

    #[test]
    fn unknown_key_translates_to_other() {
        let event = KeyEvent::new(KeyCode::CapsLock, KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::plain(Key::Other));
    }

    #[test]
    fn arrow_keys_translate_correctly() {
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            InputKeyCombo::plain(Key::Up)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            InputKeyCombo::plain(Key::Down)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            InputKeyCombo::plain(Key::Left)
        );
        assert_eq!(
            translate(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            InputKeyCombo::plain(Key::Right)
        );
    }

    #[test]
    fn function_key_translates() {
        let event = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::plain(Key::F(1)));
    }

    #[test]
    fn uppercase_char_with_shift_normalizes_to_plain() {
        for c in ['G', 'H', 'M', 'L'] {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);

            let combo = translate(event);

            assert_eq!(
                combo,
                InputKeyCombo::plain(Key::Char(c)),
                "Shift+{c} should normalize to plain {c}"
            );
        }
    }

    #[test]
    fn lowercase_char_with_shift_preserves_shift() {
        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::SHIFT);

        let combo = translate(event);

        assert_eq!(combo, InputKeyCombo::shift(Key::Char('j')),);
    }
}
