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
        use rstest::rstest;

        // Important keys with special handling: keep individual tests
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

        // Navigation keys: equivalent actions
        #[rstest]
        #[case(KeyCode::Up, "up arrow")]
        #[case(KeyCode::Char('k'), "k")]
        fn navigation_selects_previous(#[case] code: KeyCode, #[case] _desc: &str) {
            let result = handle_normal_mode(key(code));

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[rstest]
        #[case(KeyCode::Down, "down arrow")]
        #[case(KeyCode::Char('j'), "j")]
        fn navigation_selects_next(#[case] code: KeyCode, #[case] _desc: &str) {
            let result = handle_normal_mode(key(code));

            assert!(matches!(result, Action::SelectNext));
        }

        #[rstest]
        #[case(KeyCode::Char('g'), "g")]
        #[case(KeyCode::Home, "home")]
        fn navigation_selects_first(#[case] code: KeyCode, #[case] _desc: &str) {
            let result = handle_normal_mode(key(code));

            assert!(matches!(result, Action::SelectFirst));
        }

        #[rstest]
        #[case(KeyCode::Char('G'), "capital G")]
        #[case(KeyCode::End, "end")]
        fn navigation_selects_last(#[case] code: KeyCode, #[case] _desc: &str) {
            let result = handle_normal_mode(key(code));

            assert!(matches!(result, Action::SelectLast));
        }

        // Inspector tab selection (1-5 keys)
        #[rstest]
        #[case('1', InspectorTab::Columns)]
        #[case('2', InspectorTab::Indexes)]
        #[case('3', InspectorTab::ForeignKeys)]
        #[case('4', InspectorTab::Rls)]
        #[case('5', InspectorTab::Ddl)]
        fn inspector_tab_selection(#[case] key_char: char, #[case] expected_tab: InspectorTab) {
            let result = handle_normal_mode(key(KeyCode::Char(key_char)));

            assert!(matches!(result, Action::InspectorSelectTab(tab) if tab == expected_tab));
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

    mod sql_modal {
        use super::*;
        use crate::app::action::CursorMove;
        use rstest::rstest;

        // Important keys with special handling: keep individual tests
        #[test]
        fn ctrl_enter_submits_query() {
            let key = key_with_mod(KeyCode::Enter, KeyModifiers::CONTROL);

            let result = handle_sql_modal_keys(key);

            assert!(matches!(result, Action::SqlModalSubmit));
        }

        #[test]
        fn enter_without_ctrl_inserts_newline() {
            let result = handle_sql_modal_keys(key(KeyCode::Enter));

            assert!(matches!(result, Action::SqlModalNewLine));
        }

        #[test]
        fn esc_closes_modal() {
            let result = handle_sql_modal_keys(key(KeyCode::Esc));

            assert!(matches!(result, Action::CloseSqlModal));
        }

        #[test]
        fn tab_inserts_tab() {
            let result = handle_sql_modal_keys(key(KeyCode::Tab));

            assert!(matches!(result, Action::SqlModalTab));
        }

        #[test]
        fn backspace_deletes_backward() {
            let result = handle_sql_modal_keys(key(KeyCode::Backspace));

            assert!(matches!(result, Action::SqlModalBackspace));
        }

        #[test]
        fn delete_deletes_forward() {
            let result = handle_sql_modal_keys(key(KeyCode::Delete));

            assert!(matches!(result, Action::SqlModalDelete));
        }

        // Cursor movement keys
        #[rstest]
        #[case(KeyCode::Left, CursorMove::Left, "left arrow")]
        #[case(KeyCode::Right, CursorMove::Right, "right arrow")]
        #[case(KeyCode::Up, CursorMove::Up, "up arrow")]
        #[case(KeyCode::Down, CursorMove::Down, "down arrow")]
        #[case(KeyCode::Home, CursorMove::Home, "home")]
        #[case(KeyCode::End, CursorMove::End, "end")]
        fn cursor_movement(
            #[case] code: KeyCode,
            #[case] expected_move: CursorMove,
            #[case] _desc: &str,
        ) {
            let result = handle_sql_modal_keys(key(code));

            assert!(matches!(
                result,
                Action::SqlModalMoveCursor(m) if m == expected_move
            ));
        }

        #[test]
        fn char_input_inserts_character() {
            let result = handle_sql_modal_keys(key(KeyCode::Char('a')));

            assert!(matches!(result, Action::SqlModalInput('a')));
        }

        #[test]
        fn multibyte_char_input_inserts_character() {
            let result = handle_sql_modal_keys(key(KeyCode::Char('あ')));

            assert!(matches!(result, Action::SqlModalInput('あ')));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_sql_modal_keys(key(KeyCode::F(1)));

            assert!(matches!(result, Action::None));
        }
    }

    mod command_line {
        use super::*;
        use rstest::rstest;

        enum Expected {
            Submit,
            Exit,
            Backspace,
            Input(char),
            None,
        }

        #[rstest]
        #[case(KeyCode::Enter, Expected::Submit)]
        #[case(KeyCode::Esc, Expected::Exit)]
        #[case(KeyCode::Backspace, Expected::Backspace)]
        #[case(KeyCode::Char('s'), Expected::Input('s'))]
        #[case(KeyCode::Tab, Expected::None)]
        fn command_line_keys(#[case] code: KeyCode, #[case] expected: Expected) {
            let result = handle_command_line_mode(key(code));

            match expected {
                Expected::Submit => assert!(matches!(result, Action::CommandLineSubmit)),
                Expected::Exit => assert!(matches!(result, Action::ExitCommandLine)),
                Expected::Backspace => assert!(matches!(result, Action::CommandLineBackspace)),
                Expected::Input(ch) => {
                    assert!(matches!(result, Action::CommandLineInput(c) if c == ch))
                }
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod table_picker {
        use super::*;
        use rstest::rstest;

        enum Expected {
            Close,
            Confirm,
            SelectPrev,
            SelectNext,
            FilterBackspace,
            FilterInput(char),
            None,
        }

        #[rstest]
        #[case(KeyCode::Esc, Expected::Close)]
        #[case(KeyCode::Enter, Expected::Confirm)]
        #[case(KeyCode::Up, Expected::SelectPrev)]
        #[case(KeyCode::Down, Expected::SelectNext)]
        #[case(KeyCode::Backspace, Expected::FilterBackspace)]
        #[case(KeyCode::Char('u'), Expected::FilterInput('u'))]
        #[case(KeyCode::Char('日'), Expected::FilterInput('日'))]
        #[case(KeyCode::Tab, Expected::None)]
        fn table_picker_keys(#[case] code: KeyCode, #[case] expected: Expected) {
            let result = handle_table_picker_keys(key(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseTablePicker)),
                Expected::Confirm => assert!(matches!(result, Action::ConfirmSelection)),
                Expected::SelectPrev => assert!(matches!(result, Action::SelectPrevious)),
                Expected::SelectNext => assert!(matches!(result, Action::SelectNext)),
                Expected::FilterBackspace => assert!(matches!(result, Action::FilterBackspace)),
                Expected::FilterInput(ch) => {
                    assert!(matches!(result, Action::FilterInput(c) if c == ch))
                }
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod command_palette {
        use super::*;
        use rstest::rstest;

        enum Expected {
            Close,
            Confirm,
            SelectPrev,
            SelectNext,
            None,
        }

        #[rstest]
        #[case(KeyCode::Esc, Expected::Close)]
        #[case(KeyCode::Enter, Expected::Confirm)]
        #[case(KeyCode::Up, Expected::SelectPrev)]
        #[case(KeyCode::Down, Expected::SelectNext)]
        #[case(KeyCode::Char('a'), Expected::None)]
        fn command_palette_keys(#[case] code: KeyCode, #[case] expected: Expected) {
            let result = handle_command_palette_keys(key(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseCommandPalette)),
                Expected::Confirm => assert!(matches!(result, Action::ConfirmSelection)),
                Expected::SelectPrev => assert!(matches!(result, Action::SelectPrevious)),
                Expected::SelectNext => assert!(matches!(result, Action::SelectNext)),
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }
    }

    mod help {
        use super::*;

        #[test]
        fn q_quits() {
            let result = handle_help_keys(key(KeyCode::Char('q')));

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn esc_closes_help() {
            let result = handle_help_keys(key(KeyCode::Esc));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn question_mark_closes_help() {
            let result = handle_help_keys(key(KeyCode::Char('?')));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_help_keys(key(KeyCode::Char('a')));

            assert!(matches!(result, Action::None));
        }
    }

    /// Gap detection tests: spec vs implementation discrepancies
    /// These tests document features specified but not yet implemented.
    mod spec_gaps {
        use super::*;

        /// Spec: Ctrl+H should open Result History (screen_spec.md)
        /// Status: NOT IMPLEMENTED - key binding missing in handler
        #[test]
        #[ignore = "Ctrl+H Result History not implemented yet (spec gap)"]
        fn ctrl_h_should_open_result_history() {
            let key = key_with_mod(KeyCode::Char('h'), KeyModifiers::CONTROL);

            let result = handle_normal_mode(key);

            // When implemented, this should match Action::OpenResultHistory or similar
            assert!(
                !matches!(result, Action::None),
                "Ctrl+H should open Result History per spec, but returns None"
            );
        }
    }
}
