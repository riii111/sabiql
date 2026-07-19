use crate::model::app_state::AppState;
use crate::policy::FeaturePolicy;
use crate::update::action::{Action, InputTarget};
use crate::update::input::keybindings::{self, Key, KeyCombo, Modifiers};
use crate::update::input::keymap::resolve_mode_with_policy;

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

pub fn handle_settings_keys(combo: KeyCombo, state: &AppState) -> Action {
    if state.settings.is_editing_custom_er_browser() {
        return handle_custom_browser_edit_keys(combo);
    }
    if let Some(action) = keybindings::SETTINGS.resolve(&combo) {
        return action;
    }
    Action::None
}

fn handle_custom_browser_edit_keys(combo: KeyCombo) -> Action {
    use crate::update::action::CursorMove;
    match combo.key {
        Key::Enter => Action::SettingsApply,
        Key::Esc => Action::SettingsStopCustomBrowserEdit,
        Key::Char(c) => Action::TextInput {
            target: InputTarget::SettingsErBrowser,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::SettingsErBrowser,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::SettingsErBrowser,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction: CursorMove::End,
        },
        _ => Action::None,
    }
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

pub fn handle_er_table_picker_keys(combo: KeyCombo, state: &AppState) -> Action {
    let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
    if let Some(action) = resolve_mode_with_policy(
        &combo,
        keybindings::er_picker_rows(state.settings.saved_keymap_preset()),
        &feature_policy,
    ) {
        return action;
    }
    let ctrl = combo.modifiers.contains(Modifiers::CTRL);
    let alt = combo.modifiers.contains(Modifiers::ALT);
    match combo.key {
        Key::Char(c) if !ctrl || alt => Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::settings::KeymapPreset;
    use crate::update::action::ModalKind;
    use crate::update::action::{ListMotion, ListTarget};
    use crate::update::input::keybindings::{Key, KeyCombo, Modifiers};
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
            let result = handle_table_picker_keys(combo(code));

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
            let result = handle_table_picker_keys(combo_ctrl(key));

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
            let result = handle_command_palette_keys(combo(code));

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
            AppState::new("test".to_string())
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

        #[rstest]
        #[case(Key::Char('p'), ListMotion::Previous)]
        #[case(Key::Char('n'), ListMotion::Next)]
        fn ctrl_alias_selects_expected_motion(#[case] key: Key, #[case] motion: ListMotion) {
            let result = handle_query_history_picker_keys(combo_ctrl(key));

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
