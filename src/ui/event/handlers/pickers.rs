use crate::app::update::action::{Action, InputTarget};
use crate::app::update::input::keybindings::{self, Key, KeyCombo};

pub fn handle_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::TABLE_PICKER.resolve(&combo) {
        return action;
    }
    // Char input falls through to filter (keybindings resolve Backspace/Left/Right/Home/End)
    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::Filter,
            ch: c,
        },
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
        Key::Char(c) => Action::TextInput {
            target: InputTarget::QueryHistoryFilter,
            ch: c,
        },
        _ => Action::None,
    }
}

pub fn handle_er_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::ER_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::{ListMotion, ListTarget};
    use crate::app::update::input::keybindings::{Key, KeyCombo};
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
                Expected::SelectPrev => {
                    assert!(matches!(
                        result,
                        Action::ListSelect {
                            target: ListTarget::TablePicker,
                            motion: ListMotion::Previous,
                        }
                    ));
                }
                Expected::SelectNext => {
                    assert!(matches!(
                        result,
                        Action::ListSelect {
                            target: ListTarget::TablePicker,
                            motion: ListMotion::Next,
                        }
                    ));
                }
                Expected::FilterBackspace => assert!(matches!(
                    result,
                    Action::TextBackspace {
                        target: InputTarget::Filter
                    }
                )),
                Expected::FilterInput(ch) => {
                    assert!(
                        matches!(result, Action::TextInput { target: InputTarget::Filter, ch: c } if c == ch)
                    );
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
                Expected::SelectPrev => {
                    assert!(matches!(
                        result,
                        Action::ListSelect {
                            target: ListTarget::CommandPalette,
                            motion: ListMotion::Previous,
                        }
                    ));
                }
                Expected::SelectNext => {
                    assert!(matches!(
                        result,
                        Action::ListSelect {
                            target: ListTarget::CommandPalette,
                            motion: ListMotion::Next,
                        }
                    ));
                }
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod query_history_picker {
        use super::*;

        #[rstest]
        #[case(Key::Enter, Action::QueryHistoryConfirmSelection)]
        #[case(Key::Up, Action::ListSelect { target: ListTarget::QueryHistory, motion: ListMotion::Previous })]
        #[case(Key::Down, Action::ListSelect { target: ListTarget::QueryHistory, motion: ListMotion::Next })]
        #[case(Key::Backspace, Action::TextBackspace { target: InputTarget::QueryHistoryFilter })]
        #[case(Key::Esc, Action::CloseQueryHistoryPicker)]
        fn picker_keys(#[case] key: Key, #[case] expected: Action) {
            let result = handle_query_history_picker_keys(combo(key));

            assert_eq!(format!("{result:?}"), format!("{expected:?}"));
        }

        #[test]
        fn char_falls_through_to_filter_input() {
            let result = handle_query_history_picker_keys(combo(Key::Char('a')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::QueryHistoryFilter,
                    ch: 'a'
                }
            ));
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

            assert!(matches!(
                result,
                Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Previous,
                }
            ));
        }

        #[test]
        fn down_returns_select_next() {
            let result = handle_er_table_picker_keys(combo(Key::Down));

            assert!(matches!(
                result,
                Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Next,
                }
            ));
        }

        #[test]
        fn backspace_returns_er_filter_backspace() {
            let result = handle_er_table_picker_keys(combo(Key::Backspace));

            assert!(matches!(
                result,
                Action::TextBackspace {
                    target: InputTarget::ErFilter
                }
            ));
        }

        #[test]
        fn char_input_returns_er_filter_input() {
            let result = handle_er_table_picker_keys(combo(Key::Char('a')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::ErFilter,
                    ch: 'a'
                }
            ));
        }
    }
}
