use crate::app::action::Action;
use crate::app::keybindings::{self, Key, KeyCombo};
use crate::app::state::AppState;

pub fn handle_connection_setup_keys(combo: KeyCombo, state: &AppState) -> Action {
    use crate::app::action::CursorMove;
    use crate::app::connection_setup_state::ConnectionField;

    let dropdown_open = state.connection_setup.ssl_dropdown.is_open;
    let ctrl = combo.modifiers.ctrl;
    let alt = combo.modifiers.alt;

    if dropdown_open {
        return match combo.key {
            Key::Up => Action::ConnectionSetupDropdownPrev,
            Key::Down => Action::ConnectionSetupDropdownNext,
            Key::Enter => Action::ConnectionSetupDropdownConfirm,
            Key::Esc => Action::ConnectionSetupDropdownCancel,
            _ => Action::None,
        };
    }

    // Ctrl+S: save
    if ctrl && combo.key == Key::Char('s') {
        return Action::ConnectionSetupSave;
    }

    match combo.key {
        Key::Tab => Action::ConnectionSetupNextField,
        Key::BackTab => Action::ConnectionSetupPrevField,
        Key::Esc => Action::ConnectionSetupCancel,

        // SSL Mode toggle (Enter on SslMode field)
        Key::Enter if state.connection_setup.focused_field == ConnectionField::SslMode => {
            Action::ConnectionSetupToggleDropdown
        }

        // Cursor movement
        Key::Left => Action::ConnectionSetupMoveCursor(CursorMove::Left),
        Key::Right => Action::ConnectionSetupMoveCursor(CursorMove::Right),
        Key::Home => Action::ConnectionSetupMoveCursor(CursorMove::Home),
        Key::End => Action::ConnectionSetupMoveCursor(CursorMove::End),

        // Text input (allow Alt for international keyboards, block Ctrl-only)
        Key::Backspace => Action::ConnectionSetupBackspace,
        Key::Char(c) if !ctrl || alt => Action::ConnectionSetupInput(c),

        _ => Action::None,
    }
}

pub fn handle_connection_error_keys(combo: KeyCombo) -> Action {
    keybindings::CONNECTION_ERROR
        .resolve(&combo)
        .unwrap_or(Action::None)
}

pub fn handle_connection_selector_keys(combo: KeyCombo) -> Action {
    keybindings::CONNECTION_SELECTOR
        .resolve(&combo)
        .unwrap_or(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input_mode::InputMode;
    use crate::app::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn combo_alt(k: Key) -> KeyCombo {
        KeyCombo::alt(k)
    }

    mod connection_setup_keys {
        use super::*;
        use crate::app::connection_setup_state::ConnectionField;

        fn setup_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ConnectionSetup;
            state
        }

        #[test]
        fn tab_moves_to_next_field() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Tab), &state);

            assert!(matches!(result, Action::ConnectionSetupNextField));
        }

        #[test]
        fn backtab_moves_to_prev_field() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::BackTab), &state);

            assert!(matches!(result, Action::ConnectionSetupPrevField));
        }

        #[test]
        fn ctrl_s_saves() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_ctrl(Key::Char('s')), &state);

            assert!(matches!(result, Action::ConnectionSetupSave));
        }

        #[test]
        fn esc_cancels() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Esc), &state);

            assert!(matches!(result, Action::ConnectionSetupCancel));
        }

        #[test]
        fn char_input_sends_input_action() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Char('a')), &state);

            assert!(matches!(result, Action::ConnectionSetupInput('a')));
        }

        #[test]
        fn backspace_sends_backspace_action() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Backspace), &state);

            assert!(matches!(result, Action::ConnectionSetupBackspace));
        }

        #[test]
        fn ctrl_c_is_ignored() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_ctrl(Key::Char('c')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn alt_char_is_allowed_for_international_keyboards() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_alt(Key::Char('q')), &state);

            assert!(matches!(result, Action::ConnectionSetupInput('q')));
        }

        #[test]
        fn altgr_char_is_allowed() {
            use crate::app::keybindings::Modifiers;
            let state = setup_state();
            let altgr = KeyCombo {
                key: Key::Char('@'),
                modifiers: Modifiers {
                    ctrl: true,
                    alt: true,
                    shift: false,
                },
            };

            let result = handle_connection_setup_keys(altgr, &state);

            assert!(matches!(result, Action::ConnectionSetupInput('@')));
        }

        #[test]
        fn enter_on_ssl_field_toggles_dropdown() {
            let mut state = setup_state();
            state.connection_setup.focused_field = ConnectionField::SslMode;

            let result = handle_connection_setup_keys(combo(Key::Enter), &state);

            assert!(matches!(result, Action::ConnectionSetupToggleDropdown));
        }

        mod dropdown_open {
            use super::*;

            fn dropdown_state() -> AppState {
                let mut state = setup_state();
                state.connection_setup.ssl_dropdown.is_open = true;
                state
            }

            #[rstest]
            #[case(Key::Up, Action::ConnectionSetupDropdownPrev)]
            #[case(Key::Down, Action::ConnectionSetupDropdownNext)]
            #[case(Key::Enter, Action::ConnectionSetupDropdownConfirm)]
            #[case(Key::Esc, Action::ConnectionSetupDropdownCancel)]
            fn dropdown_navigation(#[case] code: Key, #[case] expected: Action) {
                let state = dropdown_state();

                let result = handle_connection_setup_keys(combo(code), &state);

                assert_eq!(
                    std::mem::discriminant(&result),
                    std::mem::discriminant(&expected)
                );
            }
        }
    }

    mod connection_error {
        use super::*;

        enum Expected {
            Close,
            Reenter,
            OpenSelector,
            ToggleDetails,
            Copy,
            ScrollUp,
            ScrollDown,
        }

        #[rstest]
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Char('e'), Expected::Reenter)]
        #[case(Key::Char('s'), Expected::OpenSelector)]
        #[case(Key::Char('d'), Expected::ToggleDetails)]
        #[case(Key::Char('c'), Expected::Copy)]
        fn connection_error_action_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_connection_error_keys(combo(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseConnectionError)),
                Expected::Reenter => assert!(matches!(result, Action::ReenterConnectionSetup)),
                Expected::OpenSelector => {
                    assert!(matches!(result, Action::OpenConnectionSelector))
                }
                Expected::ToggleDetails => {
                    assert!(matches!(result, Action::ToggleConnectionErrorDetails))
                }
                Expected::Copy => assert!(matches!(result, Action::CopyConnectionError)),
                _ => unreachable!(),
            }
        }

        #[rstest]
        #[case(Key::Up, Expected::ScrollUp)]
        #[case(Key::Char('k'), Expected::ScrollUp)]
        #[case(Key::Down, Expected::ScrollDown)]
        #[case(Key::Char('j'), Expected::ScrollDown)]
        fn connection_error_scroll_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_connection_error_keys(combo(code));

            match expected {
                Expected::ScrollUp => assert!(matches!(result, Action::ScrollConnectionErrorUp)),
                Expected::ScrollDown => {
                    assert!(matches!(result, Action::ScrollConnectionErrorDown))
                }
                _ => unreachable!(),
            }
        }

        #[test]
        fn connection_error_unbound_keys() {
            let result = handle_connection_error_keys(combo(Key::Tab));

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn r_key_retries_service_connection() {
            let result = handle_connection_error_keys(combo(Key::Char('r')));

            assert!(matches!(result, Action::RetryServiceConnection));
        }
    }

    mod connection_selector_keys {
        use super::*;

        #[rstest]
        #[case(Key::Char('j'), Action::ConnectionListSelectNext)]
        #[case(Key::Down, Action::ConnectionListSelectNext)]
        #[case(Key::Char('k'), Action::ConnectionListSelectPrevious)]
        #[case(Key::Up, Action::ConnectionListSelectPrevious)]
        fn selector_navigation_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_connection_selector_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[rstest]
        #[case(Key::Enter, Action::ConfirmConnectionSelection)]
        #[case(Key::Char('n'), Action::OpenConnectionSetup)]
        #[case(Key::Char('e'), Action::RequestEditSelectedConnection)]
        #[case(Key::Char('d'), Action::RequestDeleteSelectedConnection)]
        fn selector_action_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_connection_selector_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[test]
        fn selector_esc_closes() {
            let result = handle_connection_selector_keys(combo(Key::Esc));

            assert!(matches!(result, Action::Escape));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_connection_selector_keys(combo(Key::Char('x')));

            assert!(matches!(result, Action::None));
        }
    }
}
