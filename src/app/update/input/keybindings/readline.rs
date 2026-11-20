use crate::update::action::{Action, InputTarget};

use super::KeyCombo;
use crate::update::input::keymap;

pub fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    keymap::readline_action_for(combo, target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::inbound::Key;
    use crate::update::action::{CursorMove, TextKillDirection};
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
