use crate::model::app_state::AppState;
use crate::ports::inbound::KeyCombo;
use crate::update::action::Action;
use crate::update::input::keymap::{KeyContext, Picker, picker_action, settings_action};

pub(super) fn handle_table_picker_keys(combo: KeyCombo, state: &AppState) -> Action {
    picker_action(Picker::Table, combo, &KeyContext::from_state(state))
}

pub(super) fn handle_command_palette_keys(combo: KeyCombo, state: &AppState) -> Action {
    picker_action(
        Picker::CommandPalette,
        combo,
        &KeyContext::from_state(state),
    )
}

pub(super) fn handle_settings_keys(combo: KeyCombo, state: &AppState) -> Action {
    settings_action(combo, &KeyContext::from_state(state))
}

pub(super) fn handle_query_history_picker_keys(combo: KeyCombo, state: &AppState) -> Action {
    picker_action(Picker::QueryHistory, combo, &KeyContext::from_state(state))
}

pub(super) fn handle_er_table_picker_keys(combo: KeyCombo, state: &AppState) -> Action {
    picker_action(Picker::Er, combo, &KeyContext::from_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::settings::KeymapPreset;
    use crate::ports::inbound::{Key, KeyCombo, Modifiers};
    use crate::update::action::ModalKind;
    use crate::update::action::{InputTarget, ListMotion, ListTarget};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn combo_shift(k: Key) -> KeyCombo {
        KeyCombo::shift(k)
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
        fn handles_table_picker_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_table_picker_keys(combo(code), &AppState::new("test".to_string()));

            match expected {
                Expected::Close => {
                    assert!(matches!(result, Action::CloseModal(ModalKind::TablePicker)));
                }
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

        #[rstest]
        #[case(Key::Char('p'), ListMotion::Previous)]
        #[case(Key::Char('n'), ListMotion::Next)]
        fn ctrl_alias_selects_expected_motion(#[case] key: Key, #[case] motion: ListMotion) {
            let result =
                handle_table_picker_keys(combo_ctrl(key), &AppState::new("test".to_string()));

            assert!(matches!(
                result,
                Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: actual_motion,
                } if actual_motion == motion
            ));
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
        fn handles_command_palette_keys(#[case] code: Key, #[case] expected: Expected) {
            let result =
                handle_command_palette_keys(combo(code), &AppState::new("test".to_string()));

            match expected {
                Expected::Close => assert!(matches!(
                    result,
                    Action::CloseModal(ModalKind::CommandPalette)
                )),
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

    mod settings {
        use super::*;

        fn settings_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::Settings);
            state
        }

        fn editing_custom_browser_state() -> AppState {
            let mut state = settings_state();
            state.settings.switch_next_section();
            state.settings.switch_next_section();
            state.settings.start_custom_browser_edit();
            state
        }

        #[rstest]
        #[case(combo(Key::Enter), Action::SettingsApply)]
        #[case(combo(Key::Esc), Action::SettingsCancel)]
        #[case(combo(Key::Down), Action::SettingsSelectNext)]
        #[case(combo(Key::Up), Action::SettingsSelectPrevious)]
        #[case(combo(Key::Char('j')), Action::SettingsSelectNext)]
        #[case(combo(Key::Char('k')), Action::SettingsSelectPrevious)]
        #[case(combo_ctrl(Key::Char('n')), Action::SettingsSelectNext)]
        #[case(combo_ctrl(Key::Char('p')), Action::SettingsSelectPrevious)]
        #[case(combo(Key::Char('i')), Action::SettingsStartCustomBrowserEdit)]
        #[case(combo(Key::Tab), Action::SettingsNextSection)]
        #[case(combo_shift(Key::BackTab), Action::SettingsPreviousSection)]
        fn keys_map_to_actions(#[case] combo: KeyCombo, #[case] expected: Action) {
            let state = settings_state();
            let result = handle_settings_keys(combo, &state);

            assert_eq!(format!("{result:?}"), format!("{expected:?}"));
        }

        #[test]
        fn char_j_edits_custom_browser_when_editing() {
            let state = editing_custom_browser_state();
            let result = handle_settings_keys(combo(Key::Char('j')), &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::SettingsErBrowser,
                    ch: 'j'
                }
            ));
        }

        #[test]
        fn char_k_edits_custom_browser_when_editing() {
            let state = editing_custom_browser_state();
            let result = handle_settings_keys(combo(Key::Char('k')), &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::SettingsErBrowser,
                    ch: 'k'
                }
            ));
        }

        #[test]
        fn other_chars_edit_custom_browser_when_editing() {
            let state = editing_custom_browser_state();
            let result = handle_settings_keys(combo(Key::Char('B')), &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::SettingsErBrowser,
                    ch: 'B'
                }
            ));
        }

        #[test]
        fn esc_stops_custom_browser_editing() {
            let state = editing_custom_browser_state();
            let result = handle_settings_keys(combo(Key::Esc), &state);

            assert!(matches!(result, Action::SettingsStopCustomBrowserEdit));
        }
    }

    mod query_history_picker {
        use super::*;

        #[rstest]
        #[case(Key::Enter, Action::QueryHistoryConfirmSelection)]
        #[case(Key::Up, Action::ListSelect { target: ListTarget::QueryHistory, motion: ListMotion::Previous })]
        #[case(Key::Down, Action::ListSelect { target: ListTarget::QueryHistory, motion: ListMotion::Next })]
        #[case(Key::Backspace, Action::TextBackspace { target: InputTarget::QueryHistoryFilter })]
        #[case(Key::Esc, Action::CloseModal(ModalKind::QueryHistoryPicker))]
        fn picker_keys(#[case] key: Key, #[case] expected: Action) {
            let result =
                handle_query_history_picker_keys(combo(key), &AppState::new("test".to_string()));

            assert_eq!(format!("{result:?}"), format!("{expected:?}"));
        }

        #[test]
        fn char_falls_through_to_filter_input() {
            let result = handle_query_history_picker_keys(
                combo(Key::Char('a')),
                &AppState::new("test".to_string()),
            );

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::QueryHistoryFilter,
                    ch: 'a'
                }
            ));
        }

        #[rstest]
        #[case(Key::Char('p'), ListMotion::Previous)]
        #[case(Key::Char('n'), ListMotion::Next)]
        fn ctrl_alias_selects_expected_motion(#[case] key: Key, #[case] motion: ListMotion) {
            let result = handle_query_history_picker_keys(
                combo_ctrl(key),
                &AppState::new("test".to_string()),
            );

            assert!(matches!(
                result,
                Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: actual_motion,
                } if actual_motion == motion
            ));
        }
    }

    mod er_table_picker {
        use super::*;

        use crate::update::test_fixtures;
        fn state() -> AppState {
            let mut state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
            state
        }

        fn state_with_preset(preset: KeymapPreset) -> AppState {
            let mut state = state();
            state.settings.load_keymap_preset(preset);
            state
        }

        #[test]
        fn esc_returns_close_er_table_picker() {
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Esc), &state);

            assert!(matches!(
                result,
                Action::CloseModal(ModalKind::ErTablePicker)
            ));
        }

        #[test]
        fn enter_returns_er_confirm_selection() {
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Enter), &state);

            assert!(matches!(result, Action::ErConfirmSelection));
        }

        #[test]
        fn up_returns_select_previous() {
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Up), &state);

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
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Down), &state);

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
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Backspace), &state);

            assert!(matches!(
                result,
                Action::TextBackspace {
                    target: InputTarget::ErFilter
                }
            ));
        }

        #[test]
        fn char_input_returns_er_filter_input() {
            let state = state();
            let result = handle_er_table_picker_keys(combo(Key::Char('a')), &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::ErFilter,
                    ch: 'a'
                }
            ));
        }

        #[rstest]
        #[case(KeymapPreset::Default)]
        #[case(KeymapPreset::Ide)]
        fn alt_a_selects_all_for_both_presets(#[case] preset: KeymapPreset) {
            let state = state_with_preset(preset);
            let result = handle_er_table_picker_keys(KeyCombo::alt(Key::Char('a')), &state);

            assert!(matches!(result, Action::ErSelectAll));
        }

        #[test]
        fn ide_a_remains_filter_input() {
            let state = state_with_preset(KeymapPreset::Ide);
            let result = handle_er_table_picker_keys(combo(Key::Char('A')), &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::ErFilter,
                    ch: 'A'
                }
            ));
        }

        #[test]
        fn altgr_char_input_returns_er_filter_input() {
            let state = state();
            let altgr = KeyCombo {
                key: Key::Char('@'),
                modifiers: Modifiers::CTRL_ALT,
            };

            let result = handle_er_table_picker_keys(altgr, &state);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::ErFilter,
                    ch: '@'
                }
            ));
        }

        #[rstest]
        #[case(Key::Char('p'), ListMotion::Previous)]
        #[case(Key::Char('n'), ListMotion::Next)]
        fn ctrl_alias_selects_expected_motion(#[case] key: Key, #[case] motion: ListMotion) {
            let state = state();
            let result = handle_er_table_picker_keys(combo_ctrl(key), &state);

            assert!(matches!(
                result,
                Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: actual_motion,
                } if actual_motion == motion
            ));
        }
    }
}
