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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::inspector_tab::InspectorTab;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    mod normal_mode {
        use super::*;

        #[test]
        fn ctrl_p_opens_table_picker() {
            let key = key_with_mod(KeyCode::Char('p'), KeyModifiers::CONTROL);

            let result = handle_normal_mode(key);

            assert!(matches!(result, Action::OpenTablePicker));
        }

        #[test]
        fn ctrl_shift_p_opens_command_palette() {
            let key = key_with_mod(
                KeyCode::Char('p'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            );

            let result = handle_normal_mode(key);

            assert!(matches!(result, Action::OpenCommandPalette));
        }

        #[test]
        fn ctrl_k_opens_command_palette() {
            let key = key_with_mod(KeyCode::Char('k'), KeyModifiers::CONTROL);

            let result = handle_normal_mode(key);

            assert!(matches!(result, Action::OpenCommandPalette));
        }

        #[test]
        fn q_returns_quit() {
            let result = handle_normal_mode(key(KeyCode::Char('q')));

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn question_mark_opens_help() {
            let result = handle_normal_mode(key(KeyCode::Char('?')));

            assert!(matches!(result, Action::OpenHelp));
        }

        #[test]
        fn colon_enters_command_line() {
            let result = handle_normal_mode(key(KeyCode::Char(':')));

            assert!(matches!(result, Action::EnterCommandLine));
        }

        #[test]
        fn f_toggles_focus() {
            let result = handle_normal_mode(key(KeyCode::Char('f')));

            assert!(matches!(result, Action::ToggleFocus));
        }

        #[test]
        fn r_reloads_metadata() {
            let result = handle_normal_mode(key(KeyCode::Char('r')));

            assert!(matches!(result, Action::ReloadMetadata));
        }

        #[test]
        fn esc_returns_escape() {
            let result = handle_normal_mode(key(KeyCode::Esc));

            assert!(matches!(result, Action::Escape));
        }

        #[test]
        fn tab_returns_next_tab() {
            let result = handle_normal_mode(key(KeyCode::Tab));

            assert!(matches!(result, Action::NextTab));
        }

        #[test]
        fn shift_tab_returns_previous_tab() {
            let key = key_with_mod(KeyCode::Tab, KeyModifiers::SHIFT);

            let result = handle_normal_mode(key);

            assert!(matches!(result, Action::PreviousTab));
        }

        #[test]
        fn backtab_returns_previous_tab() {
            let result = handle_normal_mode(key(KeyCode::BackTab));

            assert!(matches!(result, Action::PreviousTab));
        }

        #[test]
        fn up_arrow_selects_previous() {
            let result = handle_normal_mode(key(KeyCode::Up));

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[test]
        fn k_selects_previous() {
            let result = handle_normal_mode(key(KeyCode::Char('k')));

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[test]
        fn down_arrow_selects_next() {
            let result = handle_normal_mode(key(KeyCode::Down));

            assert!(matches!(result, Action::SelectNext));
        }

        #[test]
        fn j_selects_next() {
            let result = handle_normal_mode(key(KeyCode::Char('j')));

            assert!(matches!(result, Action::SelectNext));
        }

        #[test]
        fn g_selects_first() {
            let result = handle_normal_mode(key(KeyCode::Char('g')));

            assert!(matches!(result, Action::SelectFirst));
        }

        #[test]
        fn capital_g_selects_last() {
            let result = handle_normal_mode(key(KeyCode::Char('G')));

            assert!(matches!(result, Action::SelectLast));
        }

        #[test]
        fn home_selects_first() {
            let result = handle_normal_mode(key(KeyCode::Home));

            assert!(matches!(result, Action::SelectFirst));
        }

        #[test]
        fn end_selects_last() {
            let result = handle_normal_mode(key(KeyCode::End));

            assert!(matches!(result, Action::SelectLast));
        }

        #[test]
        fn key_1_selects_columns_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('1')));

            assert!(matches!(
                result,
                Action::InspectorSelectTab(InspectorTab::Columns)
            ));
        }

        #[test]
        fn key_2_selects_indexes_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('2')));

            assert!(matches!(
                result,
                Action::InspectorSelectTab(InspectorTab::Indexes)
            ));
        }

        #[test]
        fn key_3_selects_foreign_keys_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('3')));

            assert!(matches!(
                result,
                Action::InspectorSelectTab(InspectorTab::ForeignKeys)
            ));
        }

        #[test]
        fn key_4_selects_rls_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('4')));

            assert!(matches!(
                result,
                Action::InspectorSelectTab(InspectorTab::Rls)
            ));
        }

        #[test]
        fn key_5_selects_ddl_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('5')));

            assert!(matches!(
                result,
                Action::InspectorSelectTab(InspectorTab::Ddl)
            ));
        }

        #[test]
        fn bracket_left_returns_inspector_prev_tab() {
            let result = handle_normal_mode(key(KeyCode::Char('[')));

            assert!(matches!(result, Action::InspectorPrevTab));
        }

        #[test]
        fn bracket_right_returns_inspector_next_tab() {
            let result = handle_normal_mode(key(KeyCode::Char(']')));

            assert!(matches!(result, Action::InspectorNextTab));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_normal_mode(key(KeyCode::Char('z')));

            assert!(matches!(result, Action::None));
        }
    }
}
