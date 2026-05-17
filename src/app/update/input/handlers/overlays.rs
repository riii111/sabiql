use crate::update::action::Action;
use crate::update::input::keybindings::{self, KeyCombo, Modifiers};
use crate::update::input::keymap;

pub fn handle_help_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::HELP.resolve(&combo) {
        return action;
    }

    match (combo.key, combo.modifiers) {
        (keybindings::Key::Char(ch), Modifiers::NONE | Modifiers::SHIFT) => Action::TextInput {
            target: crate::update::action::InputTarget::HelpFilter,
            ch,
        },
        _ => Action::None,
    }
}

pub fn handle_confirm_dialog_keys(combo: KeyCombo) -> Action {
    keymap::resolve(&combo, keybindings::CONFIRM_DIALOG_KEYS).unwrap_or(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::action::ModalKind;
    use crate::update::action::{InputTarget, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::input::keybindings::{Key, KeyCombo};
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

            assert!(matches!(result, Action::CloseModal(ModalKind::Help)));
        }

        #[test]
        fn question_mark_closes_help() {
            let result = handle_help_keys(combo(Key::Char('?')));

            assert!(matches!(result, Action::CloseModal(ModalKind::Help)));
        }

        #[test]
        fn char_falls_through_to_filter_input() {
            let result = handle_help_keys(combo(Key::Char('a')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::HelpFilter,
                    ch: 'a'
                }
            ));
        }

        #[rstest]
        #[case(KeyCombo::ctrl(Key::Char('a')))]
        #[case(KeyCombo::alt(Key::Char('a')))]
        #[case(KeyCombo::ctrl_alt(Key::Char('a')))]
        fn modified_chars_do_not_filter_help(#[case] combo: KeyCombo) {
            let result = handle_help_keys(combo);

            assert!(matches!(result, Action::None));
        }

        #[rstest]
        #[case(combo(Key::Down), ScrollDirection::Down, ScrollAmount::Line)]
        #[case(combo_ctrl(Key::Char('n')), ScrollDirection::Down, ScrollAmount::Line)]
        #[case(combo(Key::Up), ScrollDirection::Up, ScrollAmount::Line)]
        #[case(combo_ctrl(Key::Char('p')), ScrollDirection::Up, ScrollAmount::Line)]
        #[case(combo(Key::Home), ScrollDirection::Up, ScrollAmount::ToStart)]
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
        #[case(combo(Key::Left), ScrollDirection::Left, ScrollAmount::Line)]
        #[case(combo(Key::Right), ScrollDirection::Right, ScrollAmount::Line)]
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
        #[case(Key::Char('z'))]
        #[case(Key::Char('j'))]
        #[case(Key::Char('k'))]
        #[case(Key::Char('g'))]
        #[case(Key::Char('G'))]
        #[case(Key::Char('h'))]
        #[case(Key::Char('l'))]
        fn non_scroll_chars_filter_help(#[case] code: Key) {
            let result = handle_help_keys(combo(code));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::HelpFilter,
                    ..
                }
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
