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

        fn assert_help_scroll(result: Action, direction: ScrollDirection, amount: ScrollAmount) {
            assert!(matches!(
                result,
                Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: dir,
                    amount: actual_amount
                } if dir == direction && actual_amount == amount
            ));
        }

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
        #[case(combo(Key::Char('j')), ScrollDirection::Down, ScrollAmount::Line)]
        #[case(combo(Key::Down), ScrollDirection::Down, ScrollAmount::Line)]
        #[case(combo_ctrl(Key::Char('n')), ScrollDirection::Down, ScrollAmount::Line)]
        #[case(combo(Key::Char('k')), ScrollDirection::Up, ScrollAmount::Line)]
        #[case(combo(Key::Up), ScrollDirection::Up, ScrollAmount::Line)]
        #[case(combo_ctrl(Key::Char('p')), ScrollDirection::Up, ScrollAmount::Line)]
        #[case(combo(Key::Char('g')), ScrollDirection::Up, ScrollAmount::ToStart)]
        #[case(combo(Key::Home), ScrollDirection::Up, ScrollAmount::ToStart)]
        #[case(combo(Key::Char('G')), ScrollDirection::Down, ScrollAmount::ToEnd)]
        #[case(combo(Key::End), ScrollDirection::Down, ScrollAmount::ToEnd)]
        #[case(
            combo_ctrl(Key::Char('d')),
            ScrollDirection::Down,
            ScrollAmount::HalfPage
        )]
        #[case(
            combo_ctrl(Key::Char('u')),
            ScrollDirection::Up,
            ScrollAmount::HalfPage
        )]
        #[case(
            combo_ctrl(Key::Char('f')),
            ScrollDirection::Down,
            ScrollAmount::FullPage
        )]
        #[case(combo(Key::PageDown), ScrollDirection::Down, ScrollAmount::FullPage)]
        #[case(
            combo_ctrl(Key::Char('b')),
            ScrollDirection::Up,
            ScrollAmount::FullPage
        )]
        #[case(combo(Key::PageUp), ScrollDirection::Up, ScrollAmount::FullPage)]
        fn supported_help_scroll_keys_map_to_expected_action(
            #[case] combo: KeyCombo,
            #[case] direction: ScrollDirection,
            #[case] amount: ScrollAmount,
        ) {
            let result = handle_help_keys(combo);

            assert_help_scroll(result, direction, amount);
        }

        #[rstest]
        #[case(Key::Char('H'))]
        #[case(Key::Char('M'))]
        #[case(Key::Char('L'))]
        #[case(Key::Char('h'))]
        #[case(Key::Char('l'))]
        #[case(Key::Char('z'))]
        fn issue_non_goals_remain_unbound_in_help_mode(#[case] code: Key) {
            let result = handle_help_keys(combo(code));

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
        #[case(Key::Char('j'))]
        #[case(Key::Down)]
        #[case(Key::Char('n'))]
        #[case(Key::Char('k'))]
        #[case(Key::Up)]
        #[case(Key::Char('p'))]
        fn scroll_keys_return_scroll_action(#[case] code: Key) {
            let result = match code {
                Key::Char('n' | 'p') => handle_confirm_dialog_keys(combo_ctrl(code)),
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
