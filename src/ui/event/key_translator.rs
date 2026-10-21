use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::ports::inbound::{Key, KeyCombo, Modifiers};

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

    // Normalize: uppercase Key::Char already encodes shift, so drop the shift
    // flag to prevent double-encoding (e.g. Kitty sends 'G' + SHIFT).
    let raw_shift = event.modifiers.contains(KeyModifiers::SHIFT);
    let shift = match key {
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
    fn translates_basic_key_events() {
        let cases = [
            (
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                KeyCombo::plain(Key::Char('a')),
            ),
            (
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                KeyCombo::ctrl(Key::Char('p')),
            ),
            (
                KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
                KeyCombo::alt(Key::Enter),
            ),
            (
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                KeyCombo {
                    key: Key::BackTab,
                    modifiers: Modifiers::SHIFT,
                },
            ),
            (
                KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
                KeyCombo::plain(Key::Null),
            ),
            (
                KeyEvent::new(KeyCode::CapsLock, KeyModifiers::NONE),
                KeyCombo::plain(Key::Other),
            ),
            (
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                KeyCombo::plain(Key::Up),
            ),
            (
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                KeyCombo::plain(Key::Down),
            ),
            (
                KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
                KeyCombo::plain(Key::Left),
            ),
            (
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                KeyCombo::plain(Key::Right),
            ),
            (
                KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
                KeyCombo::plain(Key::F(1)),
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(translate(event), expected);
        }
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
    fn ctrl_shift_uppercase_d_normalizes_to_ctrl_uppercase_d_without_shift() {
        let event = KeyEvent::new(
            KeyCode::Char('D'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );

        let combo = translate(event);

        assert_eq!(combo, KeyCombo::ctrl(Key::Char('D')));
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
        use super::*;
        use crate::app::model::app_state::AppState;
        use crate::app::model::shared::focused_pane::FocusedPane;
        use crate::app::ports::inbound::InputEvent;
        use crate::app::update::action::{
            Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
        };
        use crate::app::update::input::handle_event;
        use crate::domain::{ConnectionId, DatabaseType};

        fn sqlite_connected_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
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
