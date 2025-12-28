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
        InputMode::SqlModal => handle_sql_modal_keys(key),
    }
}

fn handle_normal_mode(key: KeyEvent) -> Action {
    use crate::app::inspector_tab::InspectorTab;

    match (key.code, key.modifiers) {
        // Ctrl+Shift+P: Open Command Palette
        (KeyCode::Char('p'), m)
            if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
        {
            return Action::OpenCommandPalette;
        }
        // Ctrl+P: Open Table Picker (without Shift)
        (KeyCode::Char('p'), m)
            if m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::SHIFT) =>
        {
            return Action::OpenTablePicker;
        }
        // Ctrl+K: Open Command Palette (alternative)
        (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => {
            return Action::OpenCommandPalette;
        }
        // Shift+Tab: Previous tab
        (KeyCode::Tab, m) if m.contains(KeyModifiers::SHIFT) => {
            return Action::PreviousTab;
        }
        // BackTab (some terminals send this for Shift+Tab)
        (KeyCode::BackTab, _) => {
            return Action::PreviousTab;
        }
        // Tab: Next tab
        (KeyCode::Tab, _) => {
            return Action::NextTab;
        }
        _ => {}
    }

    // Regular keys
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char(':') => Action::EnterCommandLine,
        KeyCode::Char('f') => Action::ToggleFocus,
        KeyCode::Char('r') => Action::ReloadMetadata,
        KeyCode::Esc => Action::Escape,

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => Action::SelectPrevious,
        KeyCode::Down | KeyCode::Char('j') => Action::SelectNext,
        KeyCode::Char('g') => Action::SelectFirst,
        KeyCode::Char('G') => Action::SelectLast,
        KeyCode::Home => Action::SelectFirst,
        KeyCode::End => Action::SelectLast,

        // Inspector sub-tab switching (1-5 keys)
        KeyCode::Char('1') => Action::InspectorSelectTab(InspectorTab::Columns),
        KeyCode::Char('2') => Action::InspectorSelectTab(InspectorTab::Indexes),
        KeyCode::Char('3') => Action::InspectorSelectTab(InspectorTab::ForeignKeys),
        KeyCode::Char('4') => Action::InspectorSelectTab(InspectorTab::Rls),
        KeyCode::Char('5') => Action::InspectorSelectTab(InspectorTab::Ddl),

        // Inspector sub-tab navigation ([ and ])
        KeyCode::Char('[') => Action::InspectorPrevTab,
        KeyCode::Char(']') => Action::InspectorNextTab,

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
        _ => Action::None,
    }
}

fn handle_sql_modal_keys(key: KeyEvent) -> Action {
    use crate::app::action::CursorMove;

    match (key.code, key.modifiers) {
        // Ctrl+Enter: Execute query
        (KeyCode::Enter, m) if m.contains(KeyModifiers::CONTROL) => Action::SqlModalSubmit,
        // Esc: Close modal
        (KeyCode::Esc, _) => Action::CloseSqlModal,
        // Navigation
        (KeyCode::Left, _) => Action::SqlModalMoveCursor(CursorMove::Left),
        (KeyCode::Right, _) => Action::SqlModalMoveCursor(CursorMove::Right),
        (KeyCode::Up, _) => Action::SqlModalMoveCursor(CursorMove::Up),
        (KeyCode::Down, _) => Action::SqlModalMoveCursor(CursorMove::Down),
        (KeyCode::Home, _) => Action::SqlModalMoveCursor(CursorMove::Home),
        (KeyCode::End, _) => Action::SqlModalMoveCursor(CursorMove::End),
        // Editing
        (KeyCode::Backspace, _) => Action::SqlModalBackspace,
        (KeyCode::Delete, _) => Action::SqlModalDelete,
        (KeyCode::Enter, _) => Action::SqlModalNewLine,
        (KeyCode::Tab, _) => Action::SqlModalTab,
        (KeyCode::Char(c), _) => Action::SqlModalInput(c),
        _ => Action::None,
    }
}
