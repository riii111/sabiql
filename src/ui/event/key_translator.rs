use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::ports::inbound::{Key, KeyCombo, Modifiers};

pub fn translate(event: KeyEvent) -> KeyCombo {
    let mut key = match event.code {
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
        Key::BackTab => true,
        Key::Char('D')
            if event.modifiers.contains(KeyModifiers::CONTROL)
                && !event.modifiers.contains(KeyModifiers::ALT) =>
        {
            key = Key::Char('d');
            true
        }
        Key::Char(c) if c.is_ascii_uppercase() => false,
        _ => raw_shift,
    };

    let mut modifiers = Modifiers::empty();
    modifiers.set(
        Modifiers::CTRL,
        event.modifiers.contains(KeyModifiers::CONTROL),
    );
    modifiers.set(Modifiers::ALT, event.modifiers.contains(KeyModifiers::ALT));
    modifiers.set(Modifiers::SHIFT, shift);

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
    fn ctrl_shift_d_encodings_translate_to_the_same_combo() {
        let lower = KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        let upper = KeyEvent::new(KeyCode::Char('D'), KeyModifiers::CONTROL);
        assert_eq!(translate(lower), KeyCombo::ctrl_shift(Key::Char('d')));
        assert_eq!(translate(upper), translate(lower));
    }

    #[test]
    fn ctrl_alt_uppercase_d_remains_uppercase_in_text_input() {
        use crate::app::model::app_state::AppState;
        use crate::app::model::shared::input_mode::InputMode;
        use crate::app::ports::inbound::InputEvent;
        use crate::app::update::action::{Action, InputTarget};
        use crate::app::update::input::handle_event;

        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::CommandLine);
        let combo = translate(KeyEvent::new(
            KeyCode::Char('D'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        assert!(matches!(
            handle_event(InputEvent::Key(combo), &state),
            Action::TextInput {
                target: InputTarget::CommandLine,
                ch: 'D'
            }
        ));
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
                modifiers: Modifiers::SHIFT,
            }
        );
    }

    #[test]
    fn backtab_without_shift_normalizes_to_shift_backtab() {
        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE);

        assert_eq!(translate(event), KeyCombo::shift(Key::BackTab));
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

    #[test]
    fn uppercase_char_with_shift_normalizes_to_plain() {
        for c in ['G', 'H', 'M', 'L'] {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);

            let combo = translate(event);

            assert_eq!(
                combo,
                KeyCombo::plain(Key::Char(c)),
                "Shift+{c} should normalize to plain {c}"
            );
        }
    }

    #[test]
    fn lowercase_char_with_shift_preserves_shift() {
        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::SHIFT);

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::shift(Key::Char('j')),);
    }

    #[test]
    fn ctrl_shift_uppercase_d_normalizes_to_ctrl_shift_lowercase_d() {
        let event = KeyEvent::new(
            KeyCode::Char('D'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::ctrl_shift(Key::Char('d')));
    }

    #[test]
    fn ctrl_shift_lowercase_d_preserves_ctrl_shift() {
        let event = KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::ctrl_shift(Key::Char('d')));
    }

    mod sqlite_diagnostics_binding {
        use std::sync::Arc;

        use super::*;
        use crate::app::model::app_state::AppState;
        use crate::app::model::shared::focused_pane::FocusedPane;
        use crate::app::ports::inbound::InputEvent;
        use crate::app::update::action::{
            Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
        };
        use crate::app::update::input::handle_event;
        use crate::domain::connection::ConnectionEndpoint;
        use crate::domain::{ConnectionId, SqliteConnectionConfig};

        fn sqlite_connected_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "sqlite",
                &Arc::new(ConnectionEndpoint::SQLite(
                    SqliteConnectionConfig::new("sqlite:///tmp/app.db").unwrap(),
                )),
            );
            state
        }

        #[test]
        fn translated_ctrl_shift_uppercase_d_opens_diagnostics() {
            let state = sqlite_connected_state();
            let combo = translate(KeyEvent::new(
                KeyCode::Char('D'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ));

            let result = handle_event(InputEvent::Key(combo), &state);

            assert!(matches!(
                result,
                Action::OpenModal(ModalKind::SqliteDiagnostics)
            ));
        }

        #[test]
        fn translated_ctrl_d_still_half_page_scrolls_on_result() {
            let mut state = sqlite_connected_state();
            state.ui.set_focused_pane(FocusedPane::Result);
            let combo = translate(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

            let result = handle_event(InputEvent::Key(combo), &state);

            assert!(matches!(
                result,
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage
                }
            ));
        }
    }
}
