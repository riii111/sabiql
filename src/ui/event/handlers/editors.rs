use crate::app::update::action::{Action, InputTarget};
use crate::app::update::input::keybindings;
use crate::app::update::input::keybindings::{Key, KeyCombo};
use crate::app::update::input::keymap;

pub fn handle_cell_edit_keys(combo: KeyCombo) -> Action {
    use crate::app::update::action::CursorMove;
    if let Some(action) = keymap::resolve(&combo, keybindings::CELL_EDIT_KEYS) {
        return action;
    }
    match combo.key {
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::ResultCellEdit,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::ResultCellEdit,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: CursorMove::End,
        },
        Key::Char(c) => Action::TextInput {
            target: InputTarget::ResultCellEdit,
            ch: c,
        },
        _ => Action::None,
    }
}

pub fn handle_command_line_mode(combo: KeyCombo) -> Action {
    use crate::app::update::action::CursorMove;
    if let Some(action) = keymap::resolve(&combo, keybindings::COMMAND_LINE_KEYS) {
        return action;
    }
    match combo.key {
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::CommandLine,
        },
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::CommandLine,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::CommandLine,
            direction: CursorMove::Right,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::CommandLine,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::CommandLine,
            direction: CursorMove::End,
        },
        Key::Char(c) => Action::TextInput {
            target: InputTarget::CommandLine,
            ch: c,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod cell_edit_mode {
        use super::*;
        use crate::app::update::action::CursorMove;

        #[test]
        fn esc_in_cell_edit_returns_cancel_not_discard() {
            let result = handle_cell_edit_keys(combo(Key::Esc));

            assert!(matches!(result, Action::ResultCancelCellEdit));
        }

        #[test]
        fn char_input_returns_cell_edit_input() {
            let result = handle_cell_edit_keys(combo(Key::Char('x')));

            assert!(matches!(
                result,
                Action::TextInput {
                    target: InputTarget::ResultCellEdit,
                    ch: 'x'
                }
            ));
        }

        #[test]
        fn backspace_returns_cell_edit_backspace() {
            let result = handle_cell_edit_keys(combo(Key::Backspace));

            assert!(matches!(
                result,
                Action::TextBackspace {
                    target: InputTarget::ResultCellEdit
                }
            ));
        }

        #[test]
        fn delete_returns_cell_edit_delete() {
            let result = handle_cell_edit_keys(combo(Key::Delete));

            assert!(matches!(
                result,
                Action::TextDelete {
                    target: InputTarget::ResultCellEdit
                }
            ));
        }

        #[test]
        fn left_returns_move_cursor_left() {
            let result = handle_cell_edit_keys(combo(Key::Left));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Left
                }
            ));
        }

        #[test]
        fn right_returns_move_cursor_right() {
            let result = handle_cell_edit_keys(combo(Key::Right));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Right
                }
            ));
        }

        #[test]
        fn home_returns_move_cursor_home() {
            let result = handle_cell_edit_keys(combo(Key::Home));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Home
                }
            ));
        }

        #[test]
        fn end_returns_move_cursor_end() {
            let result = handle_cell_edit_keys(combo(Key::End));

            assert!(matches!(
                result,
                Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::End
                }
            ));
        }
    }

    mod command_line {
        use super::*;

        enum Expected {
            Submit,
            Exit,
            Backspace,
            Input(char),
            None,
        }

        #[rstest]
        #[case(Key::Enter, Expected::Submit)]
        #[case(Key::Esc, Expected::Exit)]
        #[case(Key::Backspace, Expected::Backspace)]
        #[case(Key::Char('s'), Expected::Input('s'))]
        #[case(Key::Tab, Expected::None)]
        fn command_line_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_command_line_mode(combo(code));

            match expected {
                Expected::Submit => assert!(matches!(result, Action::CommandLineSubmit)),
                Expected::Exit => assert!(matches!(result, Action::ExitCommandLine)),
                Expected::Backspace => assert!(matches!(
                    result,
                    Action::TextBackspace {
                        target: InputTarget::CommandLine
                    }
                )),
                Expected::Input(ch) => {
                    assert!(
                        matches!(result, Action::TextInput { target: InputTarget::CommandLine, ch: c } if c == ch)
                    );
                }
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }
}
