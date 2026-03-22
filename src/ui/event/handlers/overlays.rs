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
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
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
