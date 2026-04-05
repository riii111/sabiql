use crate::app::update::action::Action;
use crate::app::update::input::keybindings::{self, KeyCombo};
use crate::app::update::input::keymap;

pub fn handle_help_keys(combo: KeyCombo) -> Action {
    keybindings::HELP.resolve(&combo).unwrap_or(Action::None)
}

pub fn handle_confirm_dialog_keys(combo: KeyCombo) -> Action {
    keymap::resolve(&combo, keybindings::CONFIRM_DIALOG_KEYS).unwrap_or(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::{ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    mod help {
        use super::*;

        #[test]
        fn esc_closes_help() {
            let result = handle_help_keys(combo(Key::Esc));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn question_mark_closes_help() {
            let result = handle_help_keys(combo(Key::Char('?')));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_help_keys(combo(Key::Char('a')));

            assert!(matches!(result, Action::None));
        }

        #[rstest]
        #[case(Key::Char('n'), ScrollDirection::Down)]
        #[case(Key::Char('p'), ScrollDirection::Up)]
        fn ctrl_aliases_scroll(#[case] code: Key, #[case] direction: ScrollDirection) {
            let result = handle_help_keys(combo_ctrl(code));

            assert!(matches!(
                result,
                Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: dir,
                    amount: ScrollAmount::Line
                } if dir == direction
            ));
        }
    }

    mod confirm_dialog_keys {
        use super::*;

        #[rstest]
        #[case(Key::Enter, Action::ConfirmDialogConfirm)]
        #[case(Key::Esc, Action::ConfirmDialogCancel)]
        fn dialog_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_confirm_dialog_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[rstest]
        #[case(Key::Char('j'))]
        #[case(Key::Down)]
        #[case(Key::Char('n'))]
        #[case(Key::Char('k'))]
        #[case(Key::Up)]
        #[case(Key::Char('p'))]
        fn scroll_keys_return_scroll_action(#[case] code: Key) {
            let result = match code {
                Key::Char('n') | Key::Char('p') => handle_confirm_dialog_keys(combo_ctrl(code)),
                _ => handle_confirm_dialog_keys(combo(code)),
            };

            assert!(matches!(result, Action::Scroll { .. }));
        }

        #[rstest]
        #[case(Key::Char('y'))]
        #[case(Key::Char('Y'))]
        #[case(Key::Char('n'))]
        #[case(Key::Char('N'))]
        #[case(Key::Char('x'))]
        fn non_bound_keys_return_none(#[case] code: Key) {
            let result = handle_confirm_dialog_keys(combo(code));

            assert!(matches!(result, Action::None));
        }
    }
}
