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
