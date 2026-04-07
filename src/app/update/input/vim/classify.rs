use crate::app::model::shared::key_sequence::Prefix;
use crate::app::update::input::keybindings::{Key, KeyCombo};

use super::types::{SearchContinuation, VimCommand, VimModeTransition, VimNavigation, VimOperator};

pub fn classify_command(combo: &KeyCombo) -> Option<VimCommand> {
    if combo.modifiers.alt {
        return None;
    }

    if let Some(navigation) = navigation(combo) {
        return Some(VimCommand::Navigation(navigation));
    }

    if combo.modifiers.ctrl {
        return None;
    }

    match combo.key {
        Key::Esc => Some(VimCommand::ModeTransition(VimModeTransition::Escape)),
        Key::Enter => Some(VimCommand::ModeTransition(
            VimModeTransition::ConfirmOrEnter,
        )),
        Key::Char('i') => Some(VimCommand::ModeTransition(VimModeTransition::Insert)),
        Key::Char('n') => Some(VimCommand::SearchContinuation(SearchContinuation::Next)),
        Key::Char('N') => Some(VimCommand::SearchContinuation(SearchContinuation::Prev)),
        Key::Char('y') => Some(VimCommand::Operator(VimOperator::Yank)),
        Key::Char('d') => Some(VimCommand::Operator(VimOperator::Delete)),
        _ => None,
    }
}

pub fn classify_sequence(prefix: Prefix, combo: &KeyCombo) -> Option<VimCommand> {
    if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.shift {
        return None;
    }

    match prefix {
        Prefix::Z => match combo.key {
            Key::Char('z') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorCenter)),
            Key::Char('t') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorTop)),
            Key::Char('b') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorBottom)),
            _ => None,
        },
    }
}

fn navigation(combo: &KeyCombo) -> Option<VimNavigation> {
    if combo.modifiers.shift || combo.modifiers.alt {
        return None;
    }

    if combo.modifiers.ctrl {
        return match combo.key {
            Key::Char('n') => Some(VimNavigation::MoveDown),
            Key::Char('p') => Some(VimNavigation::MoveUp),
            Key::Char('d') => Some(VimNavigation::HalfPageDown),
            Key::Char('u') => Some(VimNavigation::HalfPageUp),
            Key::Char('f') => Some(VimNavigation::FullPageDown),
            Key::Char('b') => Some(VimNavigation::FullPageUp),
            _ => None,
        };
    }

    match combo.key {
        Key::Char('j') | Key::Down => Some(VimNavigation::MoveDown),
        Key::Char('k') | Key::Up => Some(VimNavigation::MoveUp),
        Key::Char('g') | Key::Home => Some(VimNavigation::MoveToFirst),
        Key::Char('G') | Key::End => Some(VimNavigation::MoveToLast),
        Key::Char('H') => Some(VimNavigation::ViewportTop),
        Key::Char('M') => Some(VimNavigation::ViewportMiddle),
        Key::Char('L') => Some(VimNavigation::ViewportBottom),
        Key::Char('h') | Key::Left => Some(VimNavigation::MoveLeft),
        Key::Char('l') | Key::Right => Some(VimNavigation::MoveRight),
        Key::PageDown => Some(VimNavigation::FullPageDown),
        Key::PageUp => Some(VimNavigation::FullPageUp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::input::keybindings::KeyCombo;
    use rstest::rstest;

    fn combo(key: Key) -> KeyCombo {
        KeyCombo::plain(key)
    }

    fn combo_ctrl(key: Key) -> KeyCombo {
        KeyCombo::ctrl(key)
    }

    #[rstest]
    #[case(Key::Char('i'), VimModeTransition::Insert)]
    #[case(Key::Enter, VimModeTransition::ConfirmOrEnter)]
    #[case(Key::Esc, VimModeTransition::Escape)]
    fn mode_transition_keys(#[case] key: Key, #[case] expected: VimModeTransition) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::ModeTransition(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('n'), SearchContinuation::Next)]
    #[case(Key::Char('N'), SearchContinuation::Prev)]
    fn search_keys(#[case] key: Key, #[case] expected: SearchContinuation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::SearchContinuation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('y'), VimOperator::Yank)]
    #[case(Key::Char('d'), VimOperator::Delete)]
    fn operator_keys(#[case] key: Key, #[case] expected: VimOperator) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Operator(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('j'), false, VimNavigation::MoveDown)]
    #[case(Key::Down, false, VimNavigation::MoveDown)]
    #[case(Key::Char('n'), true, VimNavigation::MoveDown)]
    #[case(Key::Char('k'), false, VimNavigation::MoveUp)]
    #[case(Key::Up, false, VimNavigation::MoveUp)]
    #[case(Key::Char('p'), true, VimNavigation::MoveUp)]
    fn vertical_navigation_aliases(
        #[case] key: Key,
        #[case] ctrl: bool,
        #[case] expected: VimNavigation,
    ) {
        let combo = if ctrl { combo_ctrl(key) } else { combo(key) };

        assert_eq!(
            classify_command(&combo),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('h'), VimNavigation::MoveLeft)]
    #[case(Key::Left, VimNavigation::MoveLeft)]
    #[case(Key::Char('l'), VimNavigation::MoveRight)]
    #[case(Key::Right, VimNavigation::MoveRight)]
    fn horizontal_navigation_aliases(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('g'), VimNavigation::MoveToFirst)]
    #[case(Key::Home, VimNavigation::MoveToFirst)]
    #[case(Key::Char('G'), VimNavigation::MoveToLast)]
    #[case(Key::End, VimNavigation::MoveToLast)]
    #[case(Key::Char('H'), VimNavigation::ViewportTop)]
    #[case(Key::Char('M'), VimNavigation::ViewportMiddle)]
    #[case(Key::Char('L'), VimNavigation::ViewportBottom)]
    fn boundary_navigation_aliases(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('d'), true, VimNavigation::HalfPageDown)]
    #[case(Key::Char('u'), true, VimNavigation::HalfPageUp)]
    #[case(Key::Char('f'), true, VimNavigation::FullPageDown)]
    #[case(Key::PageDown, false, VimNavigation::FullPageDown)]
    #[case(Key::Char('b'), true, VimNavigation::FullPageUp)]
    #[case(Key::PageUp, false, VimNavigation::FullPageUp)]
    fn paging_navigation_aliases(
        #[case] key: Key,
        #[case] ctrl: bool,
        #[case] expected: VimNavigation,
    ) {
        let combo = if ctrl { combo_ctrl(key) } else { combo(key) };

        assert_eq!(
            classify_command(&combo),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('z'), VimNavigation::ScrollCursorCenter)]
    #[case(Key::Char('t'), VimNavigation::ScrollCursorTop)]
    #[case(Key::Char('b'), VimNavigation::ScrollCursorBottom)]
    fn z_sequence_navigation(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_sequence(Prefix::Z, &combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }
}
