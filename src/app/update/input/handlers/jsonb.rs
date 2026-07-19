use crate::model::shared::key_sequence::Prefix;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
use crate::update::input::keybindings::{
    JSONB_DETAIL, JSONB_EDIT, JSONB_SEARCH_KEYS, Key, KeyCombo, Modifiers,
};
use crate::update::input::keymap;
use crate::update::input::vim::{
    JsonbDetailVimContext, VimSurfaceContext, action_for_input, action_for_key,
};

use super::interaction::InputInteraction;

pub fn handle_jsonb_detail_keys_with_policy(
    combo: KeyCombo,
    interaction: InputInteraction,
    pending_prefix: Option<Prefix>,
    feature_policy: &FeaturePolicy,
) -> Action {
    if !feature_policy.is_enabled(FeatureRequirement::JsonbDetail) {
        return disabled_jsonb_detail_exit_action(combo, interaction);
    }

    if matches!(
        interaction,
        InputInteraction::FormEditing(InputTarget::JsonbSearch)
    ) {
        return handle_search_input(combo, feature_policy);
    }

    if let Some(prefix) = pending_prefix {
        if combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
            return Action::CancelKeySequence;
        }
        return match action_for_input(
            &combo,
            Some(prefix),
            VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing),
        ) {
            Some(Action::None) | None => Action::CancelKeySequence,
            Some(action) => action,
        };
    }

    if !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) && combo.key == Key::Char('g')
    {
        return Action::BeginKeySequence(Prefix::G);
    }

    if !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
        match combo.key {
            Key::Home => {
                return Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineStart,
                };
            }
            Key::End => {
                return Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineEnd,
                };
            }
            _ => {}
        }
    }

    if let Some(action) = action_for_key(
        &combo,
        VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing),
    ) {
        return action;
    }

    if let Some(action) = JSONB_DETAIL.resolve_with_policy(&combo, feature_policy) {
        return action;
    }
    Action::None
}

fn handle_search_input(combo: KeyCombo, feature_policy: &FeaturePolicy) -> Action {
    // Command keys (Enter/Esc) resolved from SSOT keybindings
    if let Some(action) = keymap::resolve_with_policy(&combo, JSONB_SEARCH_KEYS, feature_policy) {
        return action;
    }
    // Text input fallthrough
    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::JsonbSearch,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::JsonbSearch,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction: CursorMove::End,
        },
        _ => Action::None,
    }
}

pub fn handle_jsonb_edit_keys_with_policy(
    combo: KeyCombo,
    feature_policy: &FeaturePolicy,
) -> Action {
    if !feature_policy.is_enabled(FeatureRequirement::JsonbDetail) {
        return if combo.modifiers.is_empty() && combo.key == Key::Esc {
            Action::JsonbExitEdit
        } else {
            Action::None
        };
    }

    if let Some(action) = action_for_key(
        &combo,
        VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Editing),
    ) {
        return action;
    }

    if let Some(action) = JSONB_EDIT.resolve_with_policy(&combo, feature_policy) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: c,
        },
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::JsonbEdit,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::JsonbEdit,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Right,
        },
        Key::Up => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Up,
        },
        Key::Down => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Down,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::End,
        },
        Key::Enter => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: '\n',
        },
        Key::Tab => Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch: '\t',
        },
        _ => Action::None,
    }
}

fn disabled_jsonb_detail_exit_action(combo: KeyCombo, interaction: InputInteraction) -> Action {
    if !combo.modifiers.is_empty() {
        return Action::None;
    }

    match (interaction, combo.key) {
        (InputInteraction::Viewing, Key::Esc | Key::Char('q')) => {
            Action::CloseModal(ModalKind::JsonbDetail)
        }
        (InputInteraction::FormEditing(InputTarget::JsonbSearch), Key::Esc) => {
            Action::JsonbExitSearch
        }
        (InputInteraction::VimEditing(InputTarget::JsonbEdit), Key::Esc) => Action::JsonbExitEdit,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
    use crate::update::action::CursorMove;
    use crate::update::input::keybindings::Key;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn handle_jsonb_detail_keys(
        combo: KeyCombo,
        interaction: InputInteraction,
        pending_prefix: Option<Prefix>,
    ) -> Action {
        let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());
        handle_jsonb_detail_keys_with_policy(combo, interaction, pending_prefix, &feature_policy)
    }

    fn handle_jsonb_edit_keys(combo: KeyCombo) -> Action {
        let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());
        handle_jsonb_edit_keys_with_policy(combo, &feature_policy)
    }

    mod jsonb_detail {
        use super::*;

        #[test]
        fn ctrl_n_moves_cursor_down_in_normal_mode() {
            let result = handle_jsonb_detail_keys(
                combo_ctrl(Key::Char('n')),
                InputInteraction::Viewing,
                None,
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                }
            ));
        }

        #[test]
        fn ctrl_p_moves_cursor_up_in_normal_mode() {
            let result = handle_jsonb_detail_keys(
                combo_ctrl(Key::Char('p')),
                InputInteraction::Viewing,
                None,
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                }
            ));
        }

        #[test]
        fn enter_is_ignored_in_viewing_mode() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Enter), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn h_moves_cursor_left_in_normal_mode() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Char('h')), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                }
            ));
        }

        #[test]
        fn home_moves_cursor_to_line_start_in_normal_mode() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Home), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineStart,
                }
            ));
        }

        #[test]
        fn end_moves_cursor_to_line_end_in_normal_mode() {
            let result = handle_jsonb_detail_keys(combo(Key::End), InputInteraction::Viewing, None);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineEnd,
                }
            ));
        }

        #[test]
        fn n_moves_to_next_search_match() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Char('n')), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::JsonbSearchNext));
        }

        #[test]
        fn upper_n_moves_to_previous_search_match() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Char('N')), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::JsonbSearchPrev));
        }

        #[test]
        fn g_begins_key_sequence() {
            let result =
                handle_jsonb_detail_keys(combo(Key::Char('g')), InputInteraction::Viewing, None);

            assert!(matches!(result, Action::BeginKeySequence(Prefix::G)));
        }

        #[test]
        fn sqlite_jsonb_detail_ignores_g() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_jsonb_detail_keys_with_policy(
                combo(Key::Char('g')),
                InputInteraction::Viewing,
                None,
                &feature_policy,
            );

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn sqlite_jsonb_detail_keeps_escape_close_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_jsonb_detail_keys_with_policy(
                combo(Key::Esc),
                InputInteraction::Viewing,
                None,
                &feature_policy,
            );

            assert!(matches!(result, Action::CloseModal(ModalKind::JsonbDetail)));
        }

        #[test]
        fn sqlite_jsonb_detail_keeps_search_escape_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_jsonb_detail_keys_with_policy(
                combo(Key::Esc),
                InputInteraction::FormEditing(InputTarget::JsonbSearch),
                None,
                &feature_policy,
            );

            assert!(matches!(result, Action::JsonbExitSearch));
        }

        #[test]
        fn gg_moves_to_first_line() {
            let result = handle_jsonb_detail_keys(
                combo(Key::Char('g')),
                InputInteraction::Viewing,
                Some(Prefix::G),
            );

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::FirstLine,
                }
            ));
        }

        #[test]
        fn unknown_prefixed_key_cancels_sequence() {
            let result = handle_jsonb_detail_keys(
                combo(Key::Char('x')),
                InputInteraction::Viewing,
                Some(Prefix::G),
            );

            assert!(matches!(result, Action::CancelKeySequence));
        }
    }

    mod jsonb_search {
        use super::*;

        #[test]
        fn ctrl_n_still_falls_through_to_search_input() {
            let result = handle_jsonb_detail_keys(
                combo_ctrl(Key::Char('n')),
                InputInteraction::FormEditing(InputTarget::JsonbSearch),
                None,
            );

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbSearch,
                    ch: 'n',
                }
            ));
        }

        #[test]
        fn ctrl_p_still_falls_through_to_search_input() {
            let result = handle_jsonb_detail_keys(
                combo_ctrl(Key::Char('p')),
                InputInteraction::FormEditing(InputTarget::JsonbSearch),
                None,
            );

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbSearch,
                    ch: 'p',
                }
            ));
        }

        #[test]
        fn pending_prefix_is_ignored_while_search_is_active() {
            let result = handle_jsonb_detail_keys(
                combo(Key::Char('g')),
                InputInteraction::FormEditing(InputTarget::JsonbSearch),
                Some(Prefix::G),
            );

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbSearch,
                    ch: 'g',
                }
            ));
        }
    }

    mod jsonb_edit {
        use super::*;
        use rstest::rstest;

        #[test]
        fn ctrl_n_still_falls_through_to_editor_input() {
            let result = handle_jsonb_edit_keys(combo_ctrl(Key::Char('n')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbEdit,
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
            let result = handle_jsonb_edit_keys(combo(key));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbEdit,
                    ch: actual_ch,
                } if actual_ch == ch
            ));
        }

        #[test]
        fn arrow_up_moves_editor_cursor() {
            let result = handle_jsonb_edit_keys(combo(Key::Up));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                }
            ));
        }

        #[test]
        fn sqlite_jsonb_edit_keeps_escape_normal_action() {
            let feature_policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

            let result = handle_jsonb_edit_keys_with_policy(combo(Key::Esc), &feature_policy);

            assert!(matches!(result, Action::JsonbExitEdit));
        }
    }
}
