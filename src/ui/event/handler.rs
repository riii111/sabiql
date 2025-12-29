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
        InputMode::Normal => handle_normal_mode(key, state),
        InputMode::CommandLine => handle_command_line_mode(key),
        InputMode::TablePicker => handle_table_picker_keys(key),
        InputMode::CommandPalette => handle_command_palette_keys(key),
        InputMode::Help => handle_help_keys(key),
        InputMode::SqlModal => {
            let completion_visible =
                state.completion.visible && !state.completion.candidates.is_empty();
            handle_sql_modal_keys(key, completion_visible)
        }
    }
}

fn handle_normal_mode(key: KeyEvent, state: &AppState) -> Action {
    use crate::app::focused_pane::FocusedPane;

    match (key.code, key.modifiers) {
        // Ctrl+P: Open Table Picker
        (KeyCode::Char('p'), m) if m.contains(KeyModifiers::CONTROL) => {
            return Action::OpenTablePicker;
        }
        // Ctrl+K: Open Command Palette
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

    let result_navigation = state.focus_mode || state.focused_pane == FocusedPane::Result;
    let inspector_navigation = state.focused_pane == FocusedPane::Inspector;

    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char(':') => Action::EnterCommandLine,
        KeyCode::Char('r') => Action::ReloadMetadata,
        KeyCode::Char('f') => Action::ToggleFocus,
        KeyCode::Esc => Action::Escape,

        KeyCode::Up | KeyCode::Char('k') => {
            if result_navigation {
                Action::ResultScrollUp
            } else if inspector_navigation {
                Action::InspectorScrollUp
            } else {
                Action::SelectPrevious
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if result_navigation {
                Action::ResultScrollDown
            } else if inspector_navigation {
                Action::InspectorScrollDown
            } else {
                Action::SelectNext
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            if result_navigation {
                Action::ResultScrollTop
            } else {
                Action::SelectFirst
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            if result_navigation {
                Action::ResultScrollBottom
            } else {
                Action::SelectLast
            }
        }

        // Horizontal scroll (h/l)
        KeyCode::Char('h') | KeyCode::Left => {
            if result_navigation {
                Action::ResultScrollLeft
            } else if inspector_navigation {
                Action::InspectorScrollLeft
            } else {
                Action::None
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if result_navigation {
                Action::ResultScrollRight
            } else if inspector_navigation {
                Action::InspectorScrollRight
            } else {
                Action::None
            }
        }

        // Pane switching: exit focus mode first if active
        KeyCode::Char(c @ '1'..='3') => {
            if state.focus_mode {
                // Exit focus mode before switching panes
                Action::ToggleFocus
            } else {
                FocusedPane::from_browse_key(c)
                    .map(Action::SetFocusedPane)
                    .unwrap_or(Action::None)
            }
        }

        // Inspector sub-tab navigation ([ and ])
        KeyCode::Char('[') => Action::InspectorPrevTab,
        KeyCode::Char(']') => Action::InspectorNextTab,

        // Console: open pgcli directly
        KeyCode::Char('c') => Action::OpenConsole,
        // SQL Modal: open adhoc query editor
        KeyCode::Char('s') => Action::OpenSqlModal,

        // Explorer: Enter to select table (only when Explorer is focused)
        KeyCode::Enter => {
            if state.focused_pane == FocusedPane::Explorer {
                Action::ConfirmSelection
            } else {
                Action::None
            }
        }

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

fn handle_sql_modal_keys(key: KeyEvent, completion_visible: bool) -> Action {
    use crate::app::action::CursorMove;

    match (key.code, key.modifiers, completion_visible) {
        // Ctrl+Enter: Execute query (priority over completion)
        (KeyCode::Enter, m, _) if m.contains(KeyModifiers::CONTROL) => Action::SqlModalSubmit,

        // Completion navigation (when popup is visible)
        (KeyCode::Up, _, true) => Action::CompletionPrev,
        (KeyCode::Down, _, true) => Action::CompletionNext,
        (KeyCode::Tab | KeyCode::Enter, _, true) => Action::CompletionAccept,
        (KeyCode::Esc, _, true) => Action::CompletionDismiss,

        // Manual completion trigger
        (KeyCode::Char(' '), m, _) if m.contains(KeyModifiers::CONTROL) => Action::CompletionTrigger,
        // Esc: Close modal (when completion not visible)
        (KeyCode::Esc, _, false) => Action::CloseSqlModal,
        // Navigation: dismiss completion on horizontal movement
        (KeyCode::Left, _, true) => Action::CompletionDismiss,
        (KeyCode::Right, _, true) => Action::CompletionDismiss,
        (KeyCode::Left, _, false) => Action::SqlModalMoveCursor(CursorMove::Left),
        (KeyCode::Right, _, false) => Action::SqlModalMoveCursor(CursorMove::Right),
        (KeyCode::Up, _, false) => Action::SqlModalMoveCursor(CursorMove::Up),
        (KeyCode::Down, _, false) => Action::SqlModalMoveCursor(CursorMove::Down),
        (KeyCode::Home, _, _) => Action::SqlModalMoveCursor(CursorMove::Home),
        (KeyCode::End, _, _) => Action::SqlModalMoveCursor(CursorMove::End),
        // Editing
        (KeyCode::Backspace, _, _) => Action::SqlModalBackspace,
        (KeyCode::Delete, _, _) => Action::SqlModalDelete,
        (KeyCode::Enter, _, _) => Action::SqlModalNewLine,
        (KeyCode::Tab, _, false) => Action::SqlModalTab,
        (KeyCode::Char(c), _, _) => Action::SqlModalInput(c),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    mod normal_mode {
        use super::*;
        use crate::app::focused_pane::FocusedPane;
        use crate::app::mode::Mode;
        use rstest::rstest;

        fn browse_state() -> AppState {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.mode = Mode::Browse;
            state
        }

        fn er_state() -> AppState {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.mode = Mode::ER;
            state
        }

        // Important keys with special handling: keep individual tests
        #[test]
        fn ctrl_p_opens_table_picker() {
            let key = key_with_mod(KeyCode::Char('p'), KeyModifiers::CONTROL);
            let state = browse_state();

            let result = handle_normal_mode(key, &state);

            assert!(matches!(result, Action::OpenTablePicker));
        }

        #[test]
        fn ctrl_k_opens_command_palette() {
            let key = key_with_mod(KeyCode::Char('k'), KeyModifiers::CONTROL);
            let state = browse_state();

            let result = handle_normal_mode(key, &state);

            assert!(matches!(result, Action::OpenCommandPalette));
        }

        #[test]
        fn q_returns_quit() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn question_mark_opens_help() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('?')), &state);

            assert!(matches!(result, Action::OpenHelp));
        }

        #[test]
        fn colon_enters_command_line() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char(':')), &state);

            assert!(matches!(result, Action::EnterCommandLine));
        }

        #[test]
        fn r_reloads_metadata() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('r')), &state);

            assert!(matches!(result, Action::ReloadMetadata));
        }

        #[test]
        fn f_toggles_focus() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('f')), &state);

            assert!(matches!(result, Action::ToggleFocus));
        }

        #[test]
        fn esc_returns_escape() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Esc), &state);

            assert!(matches!(result, Action::Escape));
        }

        #[test]
        fn tab_returns_next_tab() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Tab), &state);

            assert!(matches!(result, Action::NextTab));
        }

        #[test]
        fn shift_tab_returns_previous_tab() {
            let key = key_with_mod(KeyCode::Tab, KeyModifiers::SHIFT);
            let state = browse_state();

            let result = handle_normal_mode(key, &state);

            assert!(matches!(result, Action::PreviousTab));
        }

        #[test]
        fn backtab_returns_previous_tab() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::BackTab), &state);

            assert!(matches!(result, Action::PreviousTab));
        }

        // Navigation keys: equivalent actions
        #[rstest]
        #[case(KeyCode::Up, "up arrow")]
        #[case(KeyCode::Char('k'), "k")]
        fn navigation_selects_previous(#[case] code: KeyCode, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(key(code), &state);

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[rstest]
        #[case(KeyCode::Down, "down arrow")]
        #[case(KeyCode::Char('j'), "j")]
        fn navigation_selects_next(#[case] code: KeyCode, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(key(code), &state);

            assert!(matches!(result, Action::SelectNext));
        }

        #[rstest]
        #[case(KeyCode::Char('g'), "g")]
        #[case(KeyCode::Home, "home")]
        fn navigation_selects_first(#[case] code: KeyCode, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(key(code), &state);

            assert!(matches!(result, Action::SelectFirst));
        }

        #[rstest]
        #[case(KeyCode::Char('G'), "capital G")]
        #[case(KeyCode::End, "end")]
        fn navigation_selects_last(#[case] code: KeyCode, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(key(code), &state);

            assert!(matches!(result, Action::SelectLast));
        }

        #[test]
        fn enter_confirms_selection_when_explorer_focused() {
            let mut state = browse_state();
            state.focused_pane = FocusedPane::Explorer;

            let result = handle_normal_mode(key(KeyCode::Enter), &state);

            assert!(matches!(result, Action::ConfirmSelection));
        }

        #[test]
        fn enter_does_nothing_when_inspector_focused() {
            let mut state = browse_state();
            state.focused_pane = FocusedPane::Inspector;

            let result = handle_normal_mode(key(KeyCode::Enter), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn enter_does_nothing_when_result_focused() {
            let mut state = browse_state();
            state.focused_pane = FocusedPane::Result;

            let result = handle_normal_mode(key(KeyCode::Enter), &state);

            assert!(matches!(result, Action::None));
        }

        // Pane focus switching in Browse mode (1/2/3 keys)
        #[rstest]
        #[case('1', FocusedPane::Explorer)]
        #[case('2', FocusedPane::Inspector)]
        #[case('3', FocusedPane::Result)]
        fn browse_mode_pane_focus(#[case] key_char: char, #[case] expected_pane: FocusedPane) {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char(key_char)), &state);

            assert!(matches!(result, Action::SetFocusedPane(pane) if pane == expected_pane));
        }

        // ER mode uses Browse layout until ER view is implemented
        #[rstest]
        #[case('1', FocusedPane::Explorer)]
        #[case('2', FocusedPane::Inspector)]
        #[case('3', FocusedPane::Result)]
        fn er_mode_uses_browse_layout(#[case] key_char: char, #[case] expected_pane: FocusedPane) {
            let state = er_state();

            let result = handle_normal_mode(key(KeyCode::Char(key_char)), &state);

            assert!(matches!(result, Action::SetFocusedPane(pane) if pane == expected_pane));
        }

        #[test]
        fn bracket_left_returns_inspector_prev_tab() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('[')), &state);

            assert!(matches!(result, Action::InspectorPrevTab));
        }

        #[test]
        fn bracket_right_returns_inspector_next_tab() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char(']')), &state);

            assert!(matches!(result, Action::InspectorNextTab));
        }

        #[test]
        fn unknown_key_returns_none() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('z')), &state);

            assert!(matches!(result, Action::None));
        }

        fn focus_mode_state() -> AppState {
            let mut state = browse_state();
            state.focus_mode = true;
            state.focused_pane = FocusedPane::Result;
            state
        }

        fn result_focused_state() -> AppState {
            let mut state = browse_state();
            state.focused_pane = FocusedPane::Result;
            state
        }

        #[rstest]
        #[case(KeyCode::Char('j'))]
        #[case(KeyCode::Down)]
        fn focus_mode_j_scrolls_down(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollDown));
        }

        #[rstest]
        #[case(KeyCode::Char('k'))]
        #[case(KeyCode::Up)]
        fn focus_mode_k_scrolls_up(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollUp));
        }

        #[rstest]
        #[case(KeyCode::Char('g'))]
        #[case(KeyCode::Home)]
        fn focus_mode_g_scrolls_top(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollTop));
        }

        #[rstest]
        #[case(KeyCode::Char('G'))]
        #[case(KeyCode::End)]
        fn focus_mode_shift_g_scrolls_bottom(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollBottom));
        }

        #[rstest]
        #[case(KeyCode::Char('h'))]
        #[case(KeyCode::Left)]
        fn focus_mode_h_scrolls_left(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollLeft));
        }

        #[rstest]
        #[case(KeyCode::Char('l'))]
        #[case(KeyCode::Right)]
        fn focus_mode_l_scrolls_right(#[case] code: KeyCode) {
            let state = focus_mode_state();
            let result = handle_normal_mode(key(code), &state);
            assert!(matches!(result, Action::ResultScrollRight));
        }

        #[test]
        fn result_focused_navigation_scrolls_result() {
            let state = result_focused_state();

            let result = handle_normal_mode(key(KeyCode::Char('j')), &state);

            assert!(matches!(result, Action::ResultScrollDown));
        }

        #[test]
        fn result_focused_h_scrolls_left() {
            let state = result_focused_state();

            let result = handle_normal_mode(key(KeyCode::Char('h')), &state);

            assert!(matches!(result, Action::ResultScrollLeft));
        }

        #[test]
        fn result_focused_l_scrolls_right() {
            let state = result_focused_state();

            let result = handle_normal_mode(key(KeyCode::Char('l')), &state);

            assert!(matches!(result, Action::ResultScrollRight));
        }

        #[test]
        fn h_key_does_nothing_when_explorer_focused() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('h')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn l_key_does_nothing_when_explorer_focused() {
            let state = browse_state();

            let result = handle_normal_mode(key(KeyCode::Char('l')), &state);

            assert!(matches!(result, Action::None));
        }
    }

    mod sql_modal {
        use super::*;
        use crate::app::action::CursorMove;
        use rstest::rstest;

        #[derive(Debug, PartialEq)]
        enum Expected {
            SqlModalSubmit,
            SqlModalNewLine,
            SqlModalTab,
            SqlModalBackspace,
            SqlModalDelete,
            SqlModalInput(char),
            SqlModalMoveCursor(CursorMove),
            CloseSqlModal,
            CompletionTrigger,
            CompletionAccept,
            CompletionDismiss,
            CompletionPrev,
            CompletionNext,
            None,
        }

        fn assert_action(result: Action, expected: Expected) {
            match expected {
                Expected::SqlModalSubmit => assert!(matches!(result, Action::SqlModalSubmit)),
                Expected::SqlModalNewLine => assert!(matches!(result, Action::SqlModalNewLine)),
                Expected::SqlModalTab => assert!(matches!(result, Action::SqlModalTab)),
                Expected::SqlModalBackspace => assert!(matches!(result, Action::SqlModalBackspace)),
                Expected::SqlModalDelete => assert!(matches!(result, Action::SqlModalDelete)),
                Expected::SqlModalInput(c) => assert!(matches!(result, Action::SqlModalInput(x) if x == c)),
                Expected::SqlModalMoveCursor(m) => assert!(matches!(result, Action::SqlModalMoveCursor(x) if x == m)),
                Expected::CloseSqlModal => assert!(matches!(result, Action::CloseSqlModal)),
                Expected::CompletionTrigger => assert!(matches!(result, Action::CompletionTrigger)),
                Expected::CompletionAccept => assert!(matches!(result, Action::CompletionAccept)),
                Expected::CompletionDismiss => assert!(matches!(result, Action::CompletionDismiss)),
                Expected::CompletionPrev => assert!(matches!(result, Action::CompletionPrev)),
                Expected::CompletionNext => assert!(matches!(result, Action::CompletionNext)),
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }

        // Completion-aware keys: behavior changes based on completion visibility
        #[rstest]
        #[case(KeyCode::Esc, false, Expected::CloseSqlModal)]
        #[case(KeyCode::Esc, true, Expected::CompletionDismiss)]
        #[case(KeyCode::Tab, false, Expected::SqlModalTab)]
        #[case(KeyCode::Tab, true, Expected::CompletionAccept)]
        #[case(KeyCode::Enter, false, Expected::SqlModalNewLine)]
        #[case(KeyCode::Enter, true, Expected::CompletionAccept)]
        #[case(KeyCode::Up, false, Expected::SqlModalMoveCursor(CursorMove::Up))]
        #[case(KeyCode::Up, true, Expected::CompletionPrev)]
        #[case(KeyCode::Down, false, Expected::SqlModalMoveCursor(CursorMove::Down))]
        #[case(KeyCode::Down, true, Expected::CompletionNext)]
        fn completion_aware_keys(
            #[case] code: KeyCode,
            #[case] completion_visible: bool,
            #[case] expected: Expected,
        ) {
            let result = handle_sql_modal_keys(key(code), completion_visible);

            assert_action(result, expected);
        }

        // Keys unaffected by completion visibility
        #[rstest]
        #[case(KeyCode::Backspace, Expected::SqlModalBackspace)]
        #[case(KeyCode::Delete, Expected::SqlModalDelete)]
        #[case(KeyCode::Left, Expected::SqlModalMoveCursor(CursorMove::Left))]
        #[case(KeyCode::Right, Expected::SqlModalMoveCursor(CursorMove::Right))]
        #[case(KeyCode::Home, Expected::SqlModalMoveCursor(CursorMove::Home))]
        #[case(KeyCode::End, Expected::SqlModalMoveCursor(CursorMove::End))]
        #[case(KeyCode::F(1), Expected::None)]
        fn completion_independent_keys(#[case] code: KeyCode, #[case] expected: Expected) {
            let result = handle_sql_modal_keys(key(code), false);

            assert_action(result, expected);
        }

        #[test]
        fn ctrl_enter_submits_query() {
            let key = key_with_mod(KeyCode::Enter, KeyModifiers::CONTROL);

            let result = handle_sql_modal_keys(key, false);

            assert_action(result, Expected::SqlModalSubmit);
        }

        #[test]
        fn ctrl_space_triggers_completion() {
            let key = key_with_mod(KeyCode::Char(' '), KeyModifiers::CONTROL);

            let result = handle_sql_modal_keys(key, false);

            assert_action(result, Expected::CompletionTrigger);
        }

        #[rstest]
        #[case('a')]
        #[case('Z')]
        #[case('あ')]
        #[case('日')]
        fn char_input_inserts_character(#[case] c: char) {
            let result = handle_sql_modal_keys(key(KeyCode::Char(c)), false);

            assert_action(result, Expected::SqlModalInput(c));
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
            let state = AppState::new("test".to_string(), "default".to_string());

            let result = handle_normal_mode(key, &state);

            // When implemented, this should match Action::OpenResultHistory or similar
            assert!(
                !matches!(result, Action::None),
                "Ctrl+H should open Result History per spec, but returns None"
            );
        }
    }

    /// Smoke tests for mode dispatch: verify handle_key_event routes to correct handler
    mod mode_dispatch {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.input_mode = mode;
            state
        }

        #[test]
        fn normal_mode_routes_to_normal_handler() {
            let state = make_state(InputMode::Normal);

            // 'q' in Normal mode should quit
            let result = handle_key_event(key(KeyCode::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn sql_modal_mode_routes_to_sql_modal_handler() {
            let state = make_state(InputMode::SqlModal);

            // Esc in SqlModal should close modal (not Escape action)
            let result = handle_key_event(key(KeyCode::Esc), &state);

            assert!(matches!(result, Action::CloseSqlModal));
        }
    }
}
