use crate::app::action::Action;
use crate::app::keybindings::{self, Key, KeyCombo};

pub fn handle_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::TABLE_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::FilterInput(c),
        _ => Action::None,
    }
}

pub fn handle_command_palette_keys(combo: KeyCombo) -> Action {
    keybindings::COMMAND_PALETTE
        .resolve(&combo)
        .unwrap_or(Action::None)
}

pub fn handle_query_history_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::QUERY_HISTORY_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::QueryHistoryFilterInput(c),
        _ => Action::None,
    }
}

pub fn handle_er_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::ER_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::ErFilterInput(c),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod table_picker {
        use super::*;

        enum Expected {
            Close,
            Confirm,
            SelectPrev,
            SelectNext,
            FilterBackspace,
            FilterInput(char),
            None,
        }

        #[rstest]
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Enter, Expected::Confirm)]
        #[case(Key::Up, Expected::SelectPrev)]
        #[case(Key::Down, Expected::SelectNext)]
        #[case(Key::Backspace, Expected::FilterBackspace)]
        #[case(Key::Char('u'), Expected::FilterInput('u'))]
        #[case(Key::Char('日'), Expected::FilterInput('日'))]
        #[case(Key::Tab, Expected::None)]
        fn table_picker_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_table_picker_keys(combo(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseTablePicker)),
                Expected::Confirm => assert!(matches!(result, Action::ConfirmSelection)),
                Expected::SelectPrev => assert!(matches!(result, Action::SelectPrevious)),
                Expected::SelectNext => assert!(matches!(result, Action::SelectNext)),
                Expected::FilterBackspace => assert!(matches!(result, Action::FilterBackspace)),
                Expected::FilterInput(ch) => {
                    assert!(matches!(result, Action::FilterInput(c) if c == ch))
                }
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod command_palette {
        use super::*;

        enum Expected {
            Close,
            Confirm,
            SelectPrev,
            SelectNext,
            None,
        }

        #[rstest]
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Enter, Expected::Confirm)]
        #[case(Key::Up, Expected::SelectPrev)]
        #[case(Key::Down, Expected::SelectNext)]
        #[case(Key::Char('a'), Expected::None)]
        fn command_palette_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_command_palette_keys(combo(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseCommandPalette)),
                Expected::Confirm => assert!(matches!(result, Action::ConfirmSelection)),
                Expected::SelectPrev => assert!(matches!(result, Action::SelectPrevious)),
                Expected::SelectNext => assert!(matches!(result, Action::SelectNext)),
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod query_history_picker {
        use super::*;

        #[rstest]
        #[case(Key::Enter, Action::QueryHistoryConfirmSelection)]
        #[case(Key::Up, Action::QueryHistorySelectPrevious)]
        #[case(Key::Down, Action::QueryHistorySelectNext)]
        #[case(Key::Backspace, Action::QueryHistoryFilterBackspace)]
        #[case(Key::Esc, Action::CloseQueryHistoryPicker)]
        fn picker_keys(#[case] key: Key, #[case] expected: Action) {
            let result = handle_query_history_picker_keys(combo(key));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[test]
        fn char_falls_through_to_filter_input() {
            let result = handle_query_history_picker_keys(combo(Key::Char('a')));

            assert!(matches!(result, Action::QueryHistoryFilterInput('a')));
        }
    }

    mod er_table_picker {
        use super::*;

        #[test]
        fn esc_returns_close_er_table_picker() {
            let result = handle_er_table_picker_keys(combo(Key::Esc));

            assert!(matches!(result, Action::CloseErTablePicker));
        }

        #[test]
        fn enter_returns_er_confirm_selection() {
            let result = handle_er_table_picker_keys(combo(Key::Enter));

            assert!(matches!(result, Action::ErConfirmSelection));
        }

        #[test]
        fn up_returns_select_previous() {
            let result = handle_er_table_picker_keys(combo(Key::Up));

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[test]
        fn down_returns_select_next() {
            let result = handle_er_table_picker_keys(combo(Key::Down));

            assert!(matches!(result, Action::SelectNext));
        }

        #[test]
        fn backspace_returns_er_filter_backspace() {
            let result = handle_er_table_picker_keys(combo(Key::Backspace));

            assert!(matches!(result, Action::ErFilterBackspace));
        }

        #[test]
        fn char_input_returns_er_filter_input() {
            let result = handle_er_table_picker_keys(combo(Key::Char('a')));

            assert!(matches!(result, Action::ErFilterInput('a')));
        }
    }
}
