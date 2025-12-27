use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::action::Action;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

use super::Event;

pub fn handle_event(event: Event, state: &AppState) -> Action {
    match event {
        Event::Init => Action::Render,
        Event::Quit => Action::Quit,
        Event::Render => Action::Render,
        Event::Resize(w, h) => Action::Resize(w, h),
        Event::Key(key) => handle_key_event(key, state),
        _ => Action::None,
    }
}

fn handle_key_event(key: KeyEvent, state: &AppState) -> Action {
    match state.input_mode {
        InputMode::Normal => handle_normal_mode(key),
        InputMode::CommandLine => handle_command_line_mode(key),
        InputMode::TablePicker => handle_table_picker_keys(key),
        InputMode::CommandPalette => handle_command_palette_keys(key),
        InputMode::Help => handle_help_keys(key),
    }
}

fn handle_normal_mode(key: KeyEvent) -> Action {
    // Global keys with modifiers
    match (key.code, key.modifiers) {
        // Ctrl+P: Open Table Picker
        (KeyCode::Char('p'), m) if m.contains(KeyModifiers::CONTROL) => {
            return Action::OpenTablePicker;
        }
        // Ctrl+K: Open Command Palette
        (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => {
            return Action::OpenCommandPalette;
        }
        _ => {}
    }

    // Regular keys (no modifiers or shift only)
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char(':') => Action::EnterCommandLine,
        KeyCode::Char('1') => Action::SwitchToBrowse,
        KeyCode::Char('2') => Action::SwitchToER,
        KeyCode::Char('f') => Action::ToggleFocus,
        KeyCode::Esc => Action::Escape,

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => Action::SelectPrevious,
        KeyCode::Down | KeyCode::Char('j') => Action::SelectNext,
        KeyCode::Left | KeyCode::Char('h') => Action::Left,
        KeyCode::Right | KeyCode::Char('l') => Action::Right,
        KeyCode::Char('g') => Action::SelectFirst,
        KeyCode::Char('G') => Action::SelectLast,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home => Action::SelectFirst,
        KeyCode::End => Action::SelectLast,

        _ => Action::None,
    }
}

fn handle_command_line_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Enter => Action::CommandLineSubmit,
        KeyCode::Esc => Action::ExitCommandLine,
        KeyCode::Backspace => Action::CommandLineBackspace,
        KeyCode::Char(c) => Action::CommandLineInput(c),
        _ => Action::None,
    }
}

fn handle_table_picker_keys(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::CloseTablePicker,
        KeyCode::Enter => Action::ConfirmSelection,
        KeyCode::Up => Action::SelectPrevious,
        KeyCode::Down => Action::SelectNext,
        KeyCode::Backspace => Action::FilterBackspace,
        KeyCode::Char(c) => Action::FilterInput(c),
        _ => Action::None,
    }
}

fn handle_command_palette_keys(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::CloseCommandPalette,
        KeyCode::Enter => Action::ConfirmSelection,
        KeyCode::Up => Action::SelectPrevious,
        KeyCode::Down => Action::SelectNext,
        _ => Action::None,
    }
}

fn handle_help_keys(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc | KeyCode::Char('?') => Action::CloseHelp,
        KeyCode::Up | KeyCode::Char('k') => Action::SelectPrevious,
        KeyCode::Down | KeyCode::Char('j') => Action::SelectNext,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        _ => Action::None,
    }
}
