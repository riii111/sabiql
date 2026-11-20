use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::FeaturePolicy;
use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, InputTarget};
use crate::update::input::keymap::{
    KeyContext, json_edit_action, json_search_action, json_view_action,
};

use super::interaction::InputInteraction;

pub(super) fn handle_json_detail_keys_with_policy(
    combo: KeyCombo,
    interaction: InputInteraction,
    pending_prefix: Option<Prefix>,
    feature_policy: &FeaturePolicy,
) -> Action {
    let context =
        KeyContext::for_scope(InputMode::JsonDetail, KeymapPreset::Default, feature_policy)
            .with_prefix(pending_prefix);
    if matches!(
        interaction,
        InputInteraction::FormEditing(InputTarget::JsonSearch)
    ) {
        json_search_action(combo, &context.with_json_mode(JsonDetailMode::Searching))
    } else {
        json_view_action(combo, &context)
    }
}

pub(super) fn handle_json_edit_keys_with_policy(
    combo: KeyCombo,
    feature_policy: &FeaturePolicy,
) -> Action {
    let context = KeyContext::for_scope(InputMode::JsonEdit, KeymapPreset::Default, feature_policy)
        .with_json_edit();
    json_edit_action(combo, &context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
    use crate::update::action::{CursorMove, DetailTarget, ModalKind, SearchStep};
    use crate::update::input::keybindings::Key;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn handle_json_detail_keys(
        combo: KeyCombo,
        interaction: InputInteraction,
        pending_prefix: Option<Prefix>,
    ) -> Action {
        let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());
        handle_json_detail_keys_with_policy(combo, interaction, pending_prefix, &feature_policy)
    }

    fn handle_json_edit_keys(combo: KeyCombo) -> Action {
        let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());
        handle_json_edit_keys_with_policy(combo, &feature_policy)
    }

    mod json_detail {
        use super::*;

        #[test]
        fn ctrl_n_moves_cursor_down_in_normal_mode() {
            let result = handle_json_detail_keys(
                combo_ctrl(Key::Char('n')),
                InputInteraction::Viewing,
                None,
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Down,
                }
            ));
        }

        #[test]
        fn ctrl_p_moves_cursor_up_in_normal_mode() {
            let result = handle_json_detail_keys(
                combo_ctrl(Key::Char('p')),
                InputInteraction::Viewing,
                None,
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Up,
                }
            ));
        }

        #[test]
        fn enter_is_ignored_in_viewing_mode() {
            let result =
                handle_json_detail_keys(combo(Key::Enter), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn h_moves_cursor_left_in_normal_mode() {
            let result =
                handle_json_detail_keys(combo(Key::Char('h')), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Left,
                }
            ));
        }

        #[test]
        fn home_moves_cursor_to_line_start_in_normal_mode() {
            let result = handle_json_detail_keys(combo(Key::Home), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::LineStart,
                }
            ));
        }

        #[test]
        fn end_moves_cursor_to_line_end_in_normal_mode() {
            let result = handle_json_detail_keys(combo(Key::End), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::LineEnd,
                }
            ));
        }

        #[test]
        fn n_moves_to_next_search_match() {
            let result =
                handle_json_detail_keys(combo(Key::Char('n')), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Next,
                }
            ));
        }

        #[test]
        fn upper_n_moves_to_previous_search_match() {
            let result =
                handle_json_detail_keys(combo(Key::Char('N')), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Prev,
                }
            ));
        }

        #[test]
        fn g_begins_key_sequence() {
            let result =
                handle_json_detail_keys(combo(Key::Char('g')), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::BeginKeySequence(Prefix::G)));
        }

        #[test]
        fn sqlite_json_detail_ignores_g() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_json_detail_keys_with_policy(
                combo(Key::Char('g')),
                InputInteraction::Viewing,
                None,
                &feature_policy,
            );

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn sqlite_json_detail_keeps_escape_close_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_json_detail_keys_with_policy(
                combo(Key::Esc),
                InputInteraction::Viewing,
                None,
                &feature_policy,
            );

            assert!(matches!(result, Action::CloseModal(ModalKind::JsonDetail)));
        }

        #[test]
        fn sqlite_json_detail_keeps_search_escape_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_json_detail_keys_with_policy(
                combo(Key::Esc),
                InputInteraction::FormEditing(InputTarget::JsonSearch),
                None,
                &feature_policy,
            );

            assert!(matches!(
                result,
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Exit,
                }
            ));
        }

        #[test]
        fn mysql_json_detail_keeps_view_actions_and_enables_edit() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::mysql_like());

            assert!(matches!(
                handle_json_detail_keys_with_policy(
                    combo(Key::Char('y')),
                    InputInteraction::Viewing,
                    None,
                    &feature_policy,
                ),
                Action::DetailYankAll(DetailTarget::JsonDetail)
            ));
            assert!(matches!(
                handle_json_detail_keys_with_policy(
                    combo(Key::Char('/')),
                    InputInteraction::Viewing,
                    None,
                    &feature_policy,
                ),
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Enter,
                }
            ));
            assert!(matches!(
                handle_json_detail_keys_with_policy(
                    combo(Key::Char('i')),
                    InputInteraction::Viewing,
                    None,
                    &feature_policy,
                ),
                Action::JsonEnterEdit
            ));
        }

        #[test]
        fn gg_moves_to_first_line() {
            let result = handle_json_detail_keys(
                combo(Key::Char('g')),
                InputInteraction::Viewing,
                Some(Prefix::G),
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::FirstLine,
                }
            ));
        }

        #[test]
        fn unknown_prefixed_key_cancels_sequence() {
            let result = handle_json_detail_keys(
                combo(Key::Char('x')),
                InputInteraction::Viewing,
                Some(Prefix::G),
            );

            assert!(matches!(result, Action::CancelKeySequence));
        }
    }

    mod json_search {
        use super::*;

        #[test]
        fn ctrl_n_is_not_inserted_in_search_input() {
            let result = handle_json_detail_keys(
                combo_ctrl(Key::Char('n')),
                InputInteraction::FormEditing(InputTarget::JsonSearch),
                None,
            );

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn ctrl_p_is_not_inserted_in_search_input() {
            let result = handle_json_detail_keys(
                combo_ctrl(Key::Char('p')),
                InputInteraction::FormEditing(InputTarget::JsonSearch),
                None,
            );

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn pending_prefix_is_ignored_while_search_is_active() {
            let result = handle_json_detail_keys(
                combo(Key::Char('g')),
                InputInteraction::FormEditing(InputTarget::JsonSearch),
                Some(Prefix::G),
            );

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonSearch,
                    ch: 'g',
                }
            ));
        }
    }

    mod json_edit {
        use super::*;
        use rstest::rstest;

        #[test]
        fn ctrl_n_still_falls_through_to_editor_input() {
            let result = handle_json_edit_keys(combo_ctrl(Key::Char('n')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonEdit,
                    ch: 'n',
                }
            ));
        }

        #[rstest]
        #[case(Key::Char('i'), 'i')]
        #[case(Key::Char('d'), 'd')]
        #[case(Key::Char('n'), 'n')]
        #[case(Key::Char('h'), 'h')]
        fn vim_character_keys_still_fall_through_to_editor_input(
            #[case] key: Key,
            #[case] ch: char,
        ) {
            let result = handle_json_edit_keys(combo(key));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonEdit,
                    ch: actual_ch,
                } if actual_ch == ch
            ));
        }

        #[test]
        fn arrow_up_moves_editor_cursor() {
            let result = handle_json_edit_keys(combo(Key::Up));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Up,
                }
            ));
        }

        #[test]
        fn sqlite_json_edit_keeps_escape_normal_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_json_edit_keys_with_policy(combo(Key::Esc), &feature_policy);

            assert!(matches!(result, Action::JsonExitEdit));
        }

        #[test]
        fn mysql_json_edit_mode_accepts_editing_keys() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::mysql_like());

            assert!(matches!(
                handle_json_edit_keys_with_policy(combo(Key::Char('a')), &feature_policy),
                Action::TextInput {
                    target: InputTarget::JsonEdit,
                    ch: 'a',
                }
            ));
            assert!(matches!(
                handle_json_edit_keys_with_policy(combo(Key::Esc), &feature_policy),
                Action::JsonExitEdit
            ));
        }
    }
}
