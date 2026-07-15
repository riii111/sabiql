use crate::update::action::{Action, CursorMove, InputTarget, TextKillDirection};

use super::{Key, KeyCombo, Modifiers};

pub fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    // InputInteraction owns whether this key set edits a form field or is suppressed for a
    // Vim document editor, so surface policy does not leak into individual bindings.
    let action = match (combo.key, combo.modifiers) {
        (Key::Char('a'), Modifiers::CTRL) => Action::TextMoveCursor {
            target,
            direction: CursorMove::LineStart,
        },
        (Key::Char('e'), Modifiers::CTRL) => Action::TextMoveCursor {
            target,
            direction: CursorMove::LineEnd,
        },
        (Key::Char('b'), Modifiers::CTRL) => Action::TextMoveCursor {
            target,
            direction: CursorMove::Left,
        },
        (Key::Char('f'), Modifiers::CTRL) => Action::TextMoveCursor {
            target,
            direction: CursorMove::Right,
        },
        (Key::Char('h'), Modifiers::CTRL) => Action::TextBackspace { target },
        (Key::Char('d'), Modifiers::CTRL) => Action::TextDelete { target },
        (Key::Char('k'), Modifiers::CTRL) => Action::TextKill {
            target,
            direction: TextKillDirection::ToLineEnd,
        },
        (Key::Char('u'), Modifiers::CTRL) => Action::TextKill {
            target,
            direction: TextKillDirection::ToLineStart,
        },
        (Key::Char('w'), Modifiers::CTRL) => Action::TextKill {
            target,
            direction: TextKillDirection::ReadlinePreviousWhitespace,
        },
        (Key::Char('y'), Modifiers::CTRL) => Action::TextYank { target },
        (Key::Char('b'), Modifiers::ALT) => Action::TextMoveCursor {
            target,
            direction: CursorMove::ReadlineWordStart,
        },
        (Key::Char('f'), Modifiers::ALT) => Action::TextMoveCursor {
            target,
            direction: CursorMove::ReadlineWordEnd,
        },
        (Key::Char('d'), Modifiers::ALT) => Action::TextKill {
            target,
            direction: TextKillDirection::ReadlineWordEnd,
        },
        _ => return None,
    };

    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(KeyCombo::ctrl(Key::Char('a')), Some((Some(CursorMove::LineStart), None)))]
    #[case(KeyCombo::ctrl(Key::Char('e')), Some((Some(CursorMove::LineEnd), None)))]
    #[case(KeyCombo::ctrl(Key::Char('b')), Some((Some(CursorMove::Left), None)))]
    #[case(KeyCombo::ctrl(Key::Char('f')), Some((Some(CursorMove::Right), None)))]
    #[case(KeyCombo::ctrl(Key::Char('h')), None)]
    #[case(KeyCombo::ctrl(Key::Char('d')), None)]
    #[case(KeyCombo::ctrl(Key::Char('k')), Some((None, Some(TextKillDirection::ToLineEnd))))]
    #[case(KeyCombo::ctrl(Key::Char('u')), Some((None, Some(TextKillDirection::ToLineStart))))]
    #[case(KeyCombo::ctrl(Key::Char('w')), Some((None, Some(TextKillDirection::ReadlinePreviousWhitespace))))]
    #[case(KeyCombo::ctrl(Key::Char('y')), None)]
    #[case(KeyCombo::alt(Key::Char('b')), Some((Some(CursorMove::ReadlineWordStart), None)))]
    #[case(KeyCombo::alt(Key::Char('f')), Some((Some(CursorMove::ReadlineWordEnd), None)))]
    #[case(KeyCombo::alt(Key::Char('d')), Some((None, Some(TextKillDirection::ReadlineWordEnd))))]
    fn maps_readline_keys(
        #[case] combo: KeyCombo,
        #[case] expected: Option<(Option<CursorMove>, Option<TextKillDirection>)>,
    ) {
        let actual = readline_action_for(&combo, InputTarget::CommandLine).unwrap();

        match (expected, actual) {
            (Some((Some(expected), None)), Action::TextMoveCursor { target, direction }) => {
                assert_eq!(target, InputTarget::CommandLine);
                assert_eq!(direction, expected);
            }
            (Some((None, Some(expected))), Action::TextKill { target, direction }) => {
                assert_eq!(target, InputTarget::CommandLine);
                assert_eq!(direction, expected);
            }
            (
                None,
                Action::TextBackspace { target }
                | Action::TextDelete { target }
                | Action::TextYank { target },
            ) => assert_eq!(target, InputTarget::CommandLine),
            _ => unreachable!(),
        }
    }

    #[test]
    fn leaves_completion_navigation_unclaimed() {
        assert!(
            readline_action_for(&KeyCombo::ctrl(Key::Char('n')), InputTarget::CommandLine)
                .is_none()
        );
        assert!(
            readline_action_for(&KeyCombo::ctrl(Key::Char('p')), InputTarget::CommandLine)
                .is_none()
        );
    }
}
