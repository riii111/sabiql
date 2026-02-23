pub use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_char_key_has_no_modifiers() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        assert_eq!(event.code, KeyCode::Char('a'));
        assert_eq!(event.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn ctrl_char_key_has_control_modifier() {
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);

        assert_eq!(event.code, KeyCode::Char('p'));
        assert!(event.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn alt_enter_key_has_alt_modifier() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);

        assert_eq!(event.code, KeyCode::Enter);
        assert!(event.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn backtab_is_distinct_from_tab() {
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let backtab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);

        assert_ne!(tab.code, backtab.code);
    }

    #[test]
    fn unknown_key_roundtrips_as_null() {
        let event = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);

        assert_eq!(event.code, KeyCode::Null);
        assert_eq!(event.modifiers, KeyModifiers::NONE);
    }
}
