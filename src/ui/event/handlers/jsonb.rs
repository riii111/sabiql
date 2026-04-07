use crate::app::update::action::{Action, CursorMove, InputTarget};
use crate::app::update::input::keybindings::{
    JSONB_DETAIL, JSONB_EDIT, JSONB_SEARCH_KEYS, Key, KeyCombo,
};
use crate::app::update::input::keymap;

pub fn handle_jsonb_detail_keys(combo: KeyCombo, is_searching: bool) -> Action {
    if is_searching {
        return handle_search_input(combo);
    }

    if let Some(action) = JSONB_DETAIL.resolve(&combo) {
        return action;
    }
    Action::None
}

fn handle_search_input(combo: KeyCombo) -> Action {
    // Command keys (Enter/Esc) resolved from SSOT keybindings
    if let Some(action) = keymap::resolve(&combo, JSONB_SEARCH_KEYS) {
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

pub fn handle_jsonb_edit_keys(combo: KeyCombo) -> Action {
    if let Some(action) = JSONB_EDIT.resolve(&combo) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::CursorMove;
    use crate::app::update::input::keybindings::{Key, KeyCombo};

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    mod jsonb_detail {
        use super::*;

        #[test]
        fn ctrl_n_moves_cursor_down_in_normal_mode() {
            let result = handle_jsonb_detail_keys(combo_ctrl(Key::Char('n')), false);

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
            let result = handle_jsonb_detail_keys(combo_ctrl(Key::Char('p')), false);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                }
            ));
        }

        #[test]
        fn enter_enters_insert_mode() {
            let result = handle_jsonb_detail_keys(combo(Key::Enter), false);

            assert!(matches!(result, Action::JsonbEnterEdit));
        }

        #[test]
        fn h_moves_cursor_left_in_normal_mode() {
            let result = handle_jsonb_detail_keys(combo(Key::Char('h')), false);

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                }
            ));
        }

        #[test]
        fn n_moves_to_next_search_match() {
            let result = handle_jsonb_detail_keys(combo(Key::Char('n')), false);

            assert!(matches!(result, Action::JsonbSearchNext));
        }

        #[test]
        fn upper_n_moves_to_previous_search_match() {
            let result = handle_jsonb_detail_keys(combo(Key::Char('N')), false);

            assert!(matches!(result, Action::JsonbSearchPrev));
        }
    }

    mod jsonb_search {
        use super::*;

        #[test]
        fn ctrl_n_still_falls_through_to_search_input() {
            let result = handle_jsonb_detail_keys(combo_ctrl(Key::Char('n')), true);

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
            let result = handle_jsonb_detail_keys(combo_ctrl(Key::Char('p')), true);

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::JsonbSearch,
                    ch: 'p',
                }
            ));
        }
    }

    mod jsonb_edit {
        use super::*;

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
    }
}
