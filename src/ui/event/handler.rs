use crate::app::action::Action;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::keybindings::{Key, KeyCombo};
use crate::app::state::AppState;
use crate::app::ui_state::ResultNavMode;
use crate::app::{keybindings, keymap};

use super::Event;

pub fn handle_event(event: Event, state: &AppState) -> Action {
    match event {
        Event::Init => Action::Render,
        Event::Resize(w, h) => Action::Resize(w, h),
        Event::Key(combo) => handle_key_event(combo, state),
        Event::Paste(text) => handle_paste_event(text, state),
    }
}

fn handle_paste_event(text: String, state: &AppState) -> Action {
    match state.ui.input_mode {
        InputMode::TablePicker
        | InputMode::ErTablePicker
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::ConnectionSetup
        | InputMode::SqlModal => Action::Paste(text),
        _ => Action::None,
    }
}

fn handle_key_event(combo: KeyCombo, state: &AppState) -> Action {
    match state.ui.input_mode {
        InputMode::Normal => handle_normal_mode(combo, state),
        InputMode::CommandLine => handle_command_line_mode(combo),
        InputMode::CellEdit => handle_cell_edit_keys(combo),
        InputMode::TablePicker => handle_table_picker_keys(combo),
        InputMode::CommandPalette => handle_command_palette_keys(combo),
        InputMode::Help => handle_help_keys(combo),
        InputMode::SqlModal => {
            let completion_visible = state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty();
            handle_sql_modal_keys(combo, completion_visible)
        }
        InputMode::ConnectionSetup => handle_connection_setup_keys(combo, state),
        InputMode::ConnectionError => handle_connection_error_keys(combo),
        InputMode::ConfirmDialog => handle_confirm_dialog_keys(combo),
        InputMode::ConnectionSelector => handle_connection_selector_keys(combo),
        InputMode::ErTablePicker => handle_er_table_picker_keys(combo),
    }
}

fn handle_connection_selector_keys(combo: KeyCombo) -> Action {
    keybindings::CONNECTION_SELECTOR
        .resolve(&combo)
        .unwrap_or(Action::None)
}

fn handle_cell_edit_keys(combo: KeyCombo) -> Action {
    use crate::app::action::CursorMove;
    if let Some(action) = keymap::resolve(&combo, keybindings::CELL_EDIT_KEYS) {
        return action;
    }
    match combo.key {
        Key::Backspace => Action::ResultCellEditBackspace,
        Key::Delete => Action::ResultCellEditDelete,
        Key::Left => Action::ResultCellEditMoveCursor(CursorMove::Left),
        Key::Right => Action::ResultCellEditMoveCursor(CursorMove::Right),
        Key::Home => Action::ResultCellEditMoveCursor(CursorMove::Home),
        Key::End => Action::ResultCellEditMoveCursor(CursorMove::End),
        Key::Char(c) => Action::ResultCellEditInput(c),
        _ => Action::None,
    }
}

fn handle_normal_mode(combo: KeyCombo, state: &AppState) -> Action {
    use crate::app::focused_pane::FocusedPane;
    use keybindings as kb;

    let result_navigation = state.ui.focus_mode || state.ui.focused_pane == FocusedPane::Result;
    let inspector_navigation = state.ui.focused_pane == FocusedPane::Inspector;
    let result_nav_mode = state.ui.result_selection.mode();

    // Ctrl combos (context-independent)
    if combo.modifiers.ctrl {
        match combo.key {
            Key::Char('p') if state.query.history_index.is_none() => {
                return Action::OpenTablePicker;
            }
            Key::Char('h') => {
                return if state.query.history_index.is_some() {
                    Action::ExitResultHistory
                } else {
                    Action::OpenResultHistory
                };
            }
            Key::Char('k') if state.query.history_index.is_none() => {
                return Action::OpenCommandPalette;
            }
            Key::Char('e')
                if state
                    .query
                    .current_result
                    .as_ref()
                    .is_some_and(|r| !r.is_error()) =>
            {
                return Action::RequestCsvExport;
            }
            Key::Char('d') => {
                return if result_navigation {
                    Action::ResultScrollHalfPageDown
                } else if inspector_navigation {
                    Action::InspectorScrollHalfPageDown
                } else {
                    Action::SelectHalfPageDown
                };
            }
            Key::Char('u') => {
                return if result_navigation {
                    Action::ResultScrollHalfPageUp
                } else if inspector_navigation {
                    Action::InspectorScrollHalfPageUp
                } else {
                    Action::SelectHalfPageUp
                };
            }
            Key::Char('f') => {
                return if result_navigation {
                    Action::ResultScrollFullPageDown
                } else if inspector_navigation {
                    Action::InspectorScrollFullPageDown
                } else {
                    Action::SelectFullPageDown
                };
            }
            Key::Char('b') => {
                return if result_navigation {
                    Action::ResultScrollFullPageUp
                } else if inspector_navigation {
                    Action::InspectorScrollFullPageUp
                } else {
                    Action::SelectFullPageUp
                };
            }
            _ => {
                if state.query.history_index.is_some() {
                    return Action::None;
                }
            }
        }
    }

    // History mode: whitelist — only history nav, help, and scroll allowed
    if state.query.history_index.is_some() {
        match combo.key {
            Key::Char('[') => return Action::HistoryOlder,
            Key::Char(']') => return Action::HistoryNewer,
            Key::Char('?') => return Action::OpenHelp,
            // Scroll keys fall through to normal handling
            Key::Char('j')
            | Key::Char('k')
            | Key::Up
            | Key::Down
            | Key::Char('h')
            | Key::Char('l')
            | Key::Left
            | Key::Right
            | Key::Char('g')
            | Key::Char('G') => {}
            _ => return Action::None,
        }
    }

    // Global actions (predicate-based, no modifiers)
    if kb::is_quit(&combo) {
        return Action::Quit;
    }
    if kb::is_help(&combo) {
        return Action::OpenHelp;
    }
    if kb::is_command_line(&combo) {
        return Action::EnterCommandLine;
    }
    if kb::is_reload(&combo) {
        return Action::ReloadMetadata;
    }
    if kb::is_focus_toggle(&combo) {
        return Action::ToggleFocus;
    }

    match combo.key {
        Key::Esc => {
            if result_navigation {
                match result_nav_mode {
                    ResultNavMode::CellActive => {
                        if state.cell_edit.has_pending_draft() {
                            Action::ResultDiscardCellEdit
                        } else {
                            Action::ResultExitToRowActive
                        }
                    }
                    ResultNavMode::RowActive => Action::ResultExitToScroll,
                    ResultNavMode::Scroll => Action::Escape,
                }
            } else {
                Action::Escape
            }
        }

        Key::Up | Key::Char('k') => {
            if result_navigation {
                Action::ResultScrollUp
            } else if inspector_navigation {
                Action::InspectorScrollUp
            } else {
                Action::SelectPrevious
            }
        }
        Key::Down | Key::Char('j') => {
            if result_navigation {
                Action::ResultScrollDown
            } else if inspector_navigation {
                Action::InspectorScrollDown
            } else {
                Action::SelectNext
            }
        }
        Key::Char('g') | Key::Home => {
            if result_navigation {
                Action::ResultScrollTop
            } else if inspector_navigation {
                Action::InspectorScrollTop
            } else {
                Action::SelectFirst
            }
        }
        Key::Char('G') | Key::End => {
            if result_navigation {
                Action::ResultScrollBottom
            } else if inspector_navigation {
                Action::InspectorScrollBottom
            } else {
                Action::SelectLast
            }
        }

        Key::Char('h') | Key::Left => {
            if result_navigation && result_nav_mode == ResultNavMode::CellActive {
                Action::ResultCellLeft
            } else if result_navigation {
                Action::ResultScrollLeft
            } else if inspector_navigation {
                Action::InspectorScrollLeft
            } else if state.ui.focused_pane == FocusedPane::Explorer {
                Action::ExplorerScrollLeft
            } else {
                Action::None
            }
        }
        Key::Char('l') | Key::Right => {
            if result_navigation && result_nav_mode == ResultNavMode::CellActive {
                Action::ResultCellRight
            } else if result_navigation {
                Action::ResultScrollRight
            } else if inspector_navigation {
                Action::InspectorScrollRight
            } else if state.ui.focused_pane == FocusedPane::Explorer {
                Action::ExplorerScrollRight
            } else {
                Action::None
            }
        }

        Key::Char(']') => {
            if result_navigation {
                Action::ResultNextPage
            } else {
                Action::None
            }
        }
        Key::Char('[') => {
            if result_navigation {
                Action::ResultPrevPage
            } else {
                Action::None
            }
        }

        Key::PageDown => {
            if result_navigation {
                Action::ResultScrollFullPageDown
            } else if inspector_navigation {
                Action::InspectorScrollFullPageDown
            } else {
                Action::SelectFullPageDown
            }
        }
        Key::PageUp => {
            if result_navigation {
                Action::ResultScrollFullPageUp
            } else if inspector_navigation {
                Action::InspectorScrollFullPageUp
            } else {
                Action::SelectFullPageUp
            }
        }

        // Pane switching: exit focus mode first if active
        Key::Char(c @ '1'..='3') => {
            if state.ui.focus_mode {
                Action::ToggleFocus
            } else {
                FocusedPane::from_browse_key(c)
                    .map(Action::SetFocusedPane)
                    .unwrap_or(Action::None)
            }
        }

        // Inspector sub-tab navigation (Tab/Shift+Tab, only when Inspector focused)
        Key::Tab if inspector_navigation => Action::InspectorNextTab,
        Key::BackTab if inspector_navigation => Action::InspectorPrevTab,

        Key::Char('y') if result_navigation && result_nav_mode == ResultNavMode::CellActive => {
            Action::ResultCellYank
        }
        Key::Char('y') if inspector_navigation && state.ui.inspector_tab == InspectorTab::Ddl => {
            Action::DdlYank
        }
        Key::Char('d') if result_navigation && result_nav_mode == ResultNavMode::RowActive => {
            if state.ui.delete_op_pending {
                Action::StageRowForDelete
            } else {
                Action::ResultDeleteOperatorPending
            }
        }
        Key::Char('u') if result_navigation && result_nav_mode == ResultNavMode::RowActive => {
            Action::UnstageLastStagedRow
        }
        Key::Char('i') if result_navigation && result_nav_mode == ResultNavMode::CellActive => {
            Action::ResultEnterCellEdit
        }
        Key::Char('s') => Action::OpenSqlModal,
        Key::Char('e') => Action::OpenErTablePicker,
        Key::Char('c') if state.ui.focused_pane == FocusedPane::Explorer => {
            Action::OpenConnectionSelector
        }

        Key::Enter => {
            if state.connection_error.error_info.is_some() {
                Action::ConfirmSelection
            } else if result_navigation {
                match result_nav_mode {
                    ResultNavMode::Scroll => Action::ResultEnterRowActive,
                    ResultNavMode::RowActive => Action::ResultEnterCellActive,
                    ResultNavMode::CellActive => Action::None,
                }
            } else if state.ui.focused_pane == FocusedPane::Explorer {
                Action::ConfirmSelection
            } else {
                Action::None
            }
        }

        _ => Action::None,
    }
}

fn handle_command_line_mode(combo: KeyCombo) -> Action {
    if let Some(action) = keymap::resolve(&combo, keybindings::COMMAND_LINE_KEYS) {
        return action;
    }
    match combo.key {
        Key::Backspace => Action::CommandLineBackspace,
        Key::Char(c) => Action::CommandLineInput(c),
        _ => Action::None,
    }
}

fn handle_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::TABLE_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::FilterInput(c),
        _ => Action::None,
    }
}

fn handle_command_palette_keys(combo: KeyCombo) -> Action {
    keybindings::COMMAND_PALETTE
        .resolve(&combo)
        .unwrap_or(Action::None)
}

fn handle_help_keys(combo: KeyCombo) -> Action {
    keybindings::HELP.resolve(&combo).unwrap_or(Action::None)
}

fn handle_sql_modal_keys(combo: KeyCombo, completion_visible: bool) -> Action {
    use crate::app::action::CursorMove;

    let ctrl = combo.modifiers.ctrl;
    let alt = combo.modifiers.alt;

    // Alt+Enter: submit query
    if alt && combo.key == Key::Enter {
        return Action::SqlModalSubmit;
    }

    // Ctrl+Space: trigger completion
    if ctrl && combo.key == Key::Char(' ') {
        return Action::CompletionTrigger;
    }

    // Ctrl+L: clear
    if ctrl && combo.key == Key::Char('l') {
        return Action::SqlModalClear;
    }

    match (combo.key, completion_visible) {
        // Completion navigation (when popup is visible)
        (Key::Up, true) => Action::CompletionPrev,
        (Key::Down, true) => Action::CompletionNext,
        (Key::Tab | Key::Enter, true) => Action::CompletionAccept,
        (Key::Esc, true) => Action::CompletionDismiss,
        // Navigation: dismiss completion on horizontal movement
        (Key::Left | Key::Right, true) => Action::CompletionDismiss,

        // Esc: Close modal (when completion not visible)
        (Key::Esc, false) => Action::CloseSqlModal,
        (Key::Left, false) => Action::SqlModalMoveCursor(CursorMove::Left),
        (Key::Right, false) => Action::SqlModalMoveCursor(CursorMove::Right),
        (Key::Up, false) => Action::SqlModalMoveCursor(CursorMove::Up),
        (Key::Down, false) => Action::SqlModalMoveCursor(CursorMove::Down),
        (Key::Home, _) => Action::SqlModalMoveCursor(CursorMove::Home),
        (Key::End, _) => Action::SqlModalMoveCursor(CursorMove::End),
        // Editing
        (Key::Backspace, _) => Action::SqlModalBackspace,
        (Key::Delete, _) => Action::SqlModalDelete,
        (Key::Enter, false) => Action::SqlModalNewLine,
        (Key::Tab, false) => Action::SqlModalTab,
        (Key::Char(c), _) => Action::SqlModalInput(c),
        _ => Action::None,
    }
}

fn handle_connection_setup_keys(combo: KeyCombo, state: &AppState) -> Action {
    use crate::app::action::CursorMove;
    use crate::app::connection_setup_state::ConnectionField;

    let dropdown_open = state.connection_setup.ssl_dropdown.is_open;
    let ctrl = combo.modifiers.ctrl;
    let alt = combo.modifiers.alt;

    if dropdown_open {
        return match combo.key {
            Key::Up => Action::ConnectionSetupDropdownPrev,
            Key::Down => Action::ConnectionSetupDropdownNext,
            Key::Enter => Action::ConnectionSetupDropdownConfirm,
            Key::Esc => Action::ConnectionSetupDropdownCancel,
            _ => Action::None,
        };
    }

    // Ctrl+S: save
    if ctrl && combo.key == Key::Char('s') {
        return Action::ConnectionSetupSave;
    }

    match combo.key {
        Key::Tab => Action::ConnectionSetupNextField,
        Key::BackTab => Action::ConnectionSetupPrevField,
        Key::Esc => Action::ConnectionSetupCancel,

        // SSL Mode toggle (Enter on SslMode field)
        Key::Enter if state.connection_setup.focused_field == ConnectionField::SslMode => {
            Action::ConnectionSetupToggleDropdown
        }

        // Cursor movement
        Key::Left => Action::ConnectionSetupMoveCursor(CursorMove::Left),
        Key::Right => Action::ConnectionSetupMoveCursor(CursorMove::Right),
        Key::Home => Action::ConnectionSetupMoveCursor(CursorMove::Home),
        Key::End => Action::ConnectionSetupMoveCursor(CursorMove::End),

        // Text input (allow Alt for international keyboards, block Ctrl-only)
        Key::Backspace => Action::ConnectionSetupBackspace,
        Key::Char(c) if !ctrl || alt => Action::ConnectionSetupInput(c),

        _ => Action::None,
    }
}

fn handle_connection_error_keys(combo: KeyCombo) -> Action {
    keybindings::CONNECTION_ERROR
        .resolve(&combo)
        .unwrap_or(Action::None)
}

fn handle_er_table_picker_keys(combo: KeyCombo) -> Action {
    if let Some(action) = keybindings::ER_PICKER.resolve(&combo) {
        return action;
    }
    match combo.key {
        Key::Char(c) => Action::ErFilterInput(c),
        _ => Action::None,
    }
}

fn handle_confirm_dialog_keys(combo: KeyCombo) -> Action {
    keymap::resolve(&combo, keybindings::CONFIRM_DIALOG_KEYS).unwrap_or(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybindings::{Key, KeyCombo};

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn combo_alt(k: Key) -> KeyCombo {
        KeyCombo::alt(k)
    }

    mod normal_mode {
        use super::*;
        use crate::app::focused_pane::FocusedPane;
        use rstest::rstest;

        fn browse_state() -> AppState {
            AppState::new("test".to_string())
        }

        // Important keys with special handling: keep individual tests
        #[test]
        fn ctrl_p_opens_table_picker() {
            let state = browse_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('p')), &state);

            assert!(matches!(result, Action::OpenTablePicker));
        }

        #[test]
        fn ctrl_k_opens_command_palette() {
            let state = browse_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('k')), &state);

            assert!(matches!(result, Action::OpenCommandPalette));
        }

        #[test]
        fn q_returns_quit() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn question_mark_opens_help() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('?')), &state);

            assert!(matches!(result, Action::OpenHelp));
        }

        #[test]
        fn colon_enters_command_line() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char(':')), &state);

            assert!(matches!(result, Action::EnterCommandLine));
        }

        #[test]
        fn r_reloads_metadata() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('r')), &state);

            assert!(matches!(result, Action::ReloadMetadata));
        }

        #[test]
        fn f_toggles_focus() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('f')), &state);

            assert!(matches!(result, Action::ToggleFocus));
        }

        #[test]
        fn esc_returns_escape() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Esc), &state);

            assert!(matches!(result, Action::Escape));
        }

        // Navigation keys: equivalent actions
        #[rstest]
        #[case(Key::Up, "up arrow")]
        #[case(Key::Char('k'), "k")]
        fn navigation_selects_previous(#[case] code: Key, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[rstest]
        #[case(Key::Down, "down arrow")]
        #[case(Key::Char('j'), "j")]
        fn navigation_selects_next(#[case] code: Key, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::SelectNext));
        }

        #[rstest]
        #[case(Key::Char('g'), "g")]
        #[case(Key::Home, "home")]
        fn navigation_selects_first(#[case] code: Key, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::SelectFirst));
        }

        #[rstest]
        #[case(Key::Char('G'), "capital G")]
        #[case(Key::End, "end")]
        fn navigation_selects_last(#[case] code: Key, #[case] _desc: &str) {
            let state = browse_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::SelectLast));
        }

        #[test]
        fn enter_confirms_selection_when_explorer_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Explorer;

            let result = handle_normal_mode(combo(Key::Enter), &state);

            assert!(matches!(result, Action::ConfirmSelection));
        }

        #[test]
        fn enter_does_nothing_when_inspector_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Inspector;

            let result = handle_normal_mode(combo(Key::Enter), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn enter_enters_row_active_when_result_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Result;

            let result = handle_normal_mode(combo(Key::Enter), &state);

            assert!(matches!(result, Action::ResultEnterRowActive));
        }

        // Pane focus switching in Browse mode (1/2/3 keys)
        #[rstest]
        #[case('1', FocusedPane::Explorer)]
        #[case('2', FocusedPane::Inspector)]
        #[case('3', FocusedPane::Result)]
        fn browse_mode_pane_focus(#[case] key_char: char, #[case] expected_pane: FocusedPane) {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char(key_char)), &state);

            assert!(matches!(result, Action::SetFocusedPane(pane) if pane == expected_pane));
        }

        #[test]
        fn tab_switches_inspector_tab_when_inspector_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Inspector;

            let result = handle_normal_mode(combo(Key::Tab), &state);

            assert!(matches!(result, Action::InspectorNextTab));
        }

        #[test]
        fn shift_tab_switches_inspector_tab_prev_when_inspector_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Inspector;

            let result = handle_normal_mode(combo(Key::BackTab), &state);

            assert!(matches!(result, Action::InspectorPrevTab));
        }

        #[test]
        fn tab_does_nothing_when_explorer_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Explorer;

            let result = handle_normal_mode(combo(Key::Tab), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn tab_does_nothing_when_result_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Result;

            let result = handle_normal_mode(combo(Key::Tab), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn backtab_does_nothing_when_explorer_focused() {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Explorer;

            let result = handle_normal_mode(combo(Key::BackTab), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn unknown_key_returns_none() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(result, Action::None));
        }

        fn focus_mode_state() -> AppState {
            let mut state = browse_state();
            state.ui.focus_mode = true;
            state.ui.focused_pane = FocusedPane::Result;
            state
        }

        fn result_focused_state() -> AppState {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Result;
            state
        }

        #[rstest]
        #[case(Key::Char('j'))]
        #[case(Key::Down)]
        fn focus_mode_j_scrolls_down(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollDown));
        }

        #[rstest]
        #[case(Key::Char('k'))]
        #[case(Key::Up)]
        fn focus_mode_k_scrolls_up(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollUp));
        }

        #[rstest]
        #[case(Key::Char('g'))]
        #[case(Key::Home)]
        fn focus_mode_g_scrolls_top(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollTop));
        }

        #[rstest]
        #[case(Key::Char('G'))]
        #[case(Key::End)]
        fn focus_mode_shift_g_scrolls_bottom(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollBottom));
        }

        #[rstest]
        #[case(Key::Char('h'))]
        #[case(Key::Left)]
        fn focus_mode_h_scrolls_left(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollLeft));
        }

        #[rstest]
        #[case(Key::Char('l'))]
        #[case(Key::Right)]
        fn focus_mode_l_scrolls_right(#[case] code: Key) {
            let state = focus_mode_state();
            let result = handle_normal_mode(combo(code), &state);
            assert!(matches!(result, Action::ResultScrollRight));
        }

        #[test]
        fn result_focused_navigation_scrolls_result() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('j')), &state);

            assert!(matches!(result, Action::ResultScrollDown));
        }

        #[test]
        fn result_focused_h_scrolls_left() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('h')), &state);

            assert!(matches!(result, Action::ResultScrollLeft));
        }

        #[test]
        fn result_focused_l_scrolls_right() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('l')), &state);

            assert!(matches!(result, Action::ResultScrollRight));
        }

        #[test]
        fn d_sets_delete_op_pending_in_row_active() {
            let mut state = result_focused_state();
            state.ui.result_selection.enter_row(0);

            let result = handle_normal_mode(combo(Key::Char('d')), &state);

            assert!(matches!(result, Action::ResultDeleteOperatorPending));
        }

        #[test]
        fn dd_stages_row_for_delete_in_row_active() {
            let mut state = result_focused_state();
            state.ui.result_selection.enter_row(0);
            state.ui.delete_op_pending = true;

            let result = handle_normal_mode(combo(Key::Char('d')), &state);

            assert!(matches!(result, Action::StageRowForDelete));
        }

        #[test]
        fn d_in_scroll_mode_is_noop() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('d')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn h_key_scrolls_left_when_explorer_focused() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('h')), &state);

            assert!(matches!(result, Action::ExplorerScrollLeft));
        }

        #[test]
        fn l_key_scrolls_right_when_explorer_focused() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('l')), &state);

            assert!(matches!(result, Action::ExplorerScrollRight));
        }

        #[test]
        fn e_key_opens_er_table_picker() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('e')), &state);

            assert!(matches!(result, Action::OpenErTablePicker));
        }

        #[test]
        fn esc_in_cell_active_with_draft_returns_discard() {
            let mut state = result_focused_state();
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);
            state.cell_edit.begin(0, 1, "original".to_string());
            state.cell_edit.input.set_content("modified".to_string());

            let result = handle_normal_mode(combo(Key::Esc), &state);

            assert!(matches!(result, Action::ResultDiscardCellEdit));
        }

        #[test]
        fn esc_in_cell_active_without_draft_returns_exit_to_row_active() {
            let mut state = result_focused_state();
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);

            let result = handle_normal_mode(combo(Key::Esc), &state);

            assert!(matches!(result, Action::ResultExitToRowActive));
        }

        #[test]
        fn i_key_enters_cell_edit_when_cell_active() {
            let mut state = result_focused_state();
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);

            let result = handle_normal_mode(combo(Key::Char('i')), &state);

            assert!(matches!(result, Action::ResultEnterCellEdit));
        }

        #[test]
        fn i_key_is_noop_without_cell_active() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('i')), &state);

            assert!(matches!(result, Action::None));
        }

        fn inspector_focused_state() -> AppState {
            let mut state = browse_state();
            state.ui.focused_pane = FocusedPane::Inspector;
            state
        }

        #[rstest]
        #[case(Key::Char('g'))]
        #[case(Key::Home)]
        fn g_scrolls_inspector_top(#[case] code: Key) {
            let state = inspector_focused_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::InspectorScrollTop));
        }

        #[rstest]
        #[case(Key::Char('G'))]
        #[case(Key::End)]
        fn shift_g_scrolls_inspector_bottom(#[case] code: Key) {
            let state = inspector_focused_state();

            let result = handle_normal_mode(combo(code), &state);

            assert!(matches!(result, Action::InspectorScrollBottom));
        }

        #[test]
        fn c_key_opens_connection_selector() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('c')), &state);

            assert!(matches!(result, Action::OpenConnectionSelector));
        }

        #[test]
        fn c_key_noop_when_result_focused() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('c')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn c_key_noop_when_inspector_focused() {
            let state = inspector_focused_state();

            let result = handle_normal_mode(combo(Key::Char('c')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn bracket_right_returns_next_page_when_result_focused() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char(']')), &state);

            assert!(matches!(result, Action::ResultNextPage));
        }

        #[test]
        fn bracket_left_returns_prev_page_when_result_focused() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(result, Action::ResultPrevPage));
        }

        #[test]
        fn brackets_return_none_when_explorer_focused() {
            let state = browse_state();

            let right = handle_normal_mode(combo(Key::Char(']')), &state);
            let left = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(right, Action::None));
            assert!(matches!(left, Action::None));
        }

        #[test]
        fn bracket_right_returns_next_page_in_focus_mode() {
            let state = focus_mode_state();

            let result = handle_normal_mode(combo(Key::Char(']')), &state);

            assert!(matches!(result, Action::ResultNextPage));
        }

        #[test]
        fn bracket_left_returns_prev_page_in_focus_mode() {
            let state = focus_mode_state();

            let result = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(result, Action::ResultPrevPage));
        }

        // Page scroll: Ctrl-D/U/F/B and PageDown/PageUp
        #[test]
        fn ctrl_d_scrolls_result_half_page_down() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

            assert!(matches!(result, Action::ResultScrollHalfPageDown));
        }

        #[test]
        fn ctrl_u_scrolls_result_half_page_up() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('u')), &state);

            assert!(matches!(result, Action::ResultScrollHalfPageUp));
        }

        #[test]
        fn ctrl_f_scrolls_result_full_page_down() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('f')), &state);

            assert!(matches!(result, Action::ResultScrollFullPageDown));
        }

        #[test]
        fn ctrl_b_scrolls_result_full_page_up() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('b')), &state);

            assert!(matches!(result, Action::ResultScrollFullPageUp));
        }

        #[test]
        fn ctrl_d_scrolls_inspector_half_page_down() {
            let state = inspector_focused_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

            assert!(matches!(result, Action::InspectorScrollHalfPageDown));
        }

        #[test]
        fn ctrl_d_scrolls_explorer_half_page_down() {
            let state = browse_state();

            let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

            assert!(matches!(result, Action::SelectHalfPageDown));
        }

        #[test]
        fn pagedown_scrolls_result_full_page_down() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::PageDown), &state);

            assert!(matches!(result, Action::ResultScrollFullPageDown));
        }

        #[test]
        fn pageup_scrolls_result_full_page_up() {
            let state = result_focused_state();

            let result = handle_normal_mode(combo(Key::PageUp), &state);

            assert!(matches!(result, Action::ResultScrollFullPageUp));
        }

        #[test]
        fn pagedown_scrolls_inspector_full_page_down() {
            let state = inspector_focused_state();

            let result = handle_normal_mode(combo(Key::PageDown), &state);

            assert!(matches!(result, Action::InspectorScrollFullPageDown));
        }

        #[test]
        fn pagedown_scrolls_explorer_full_page_down() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::PageDown), &state);

            assert!(matches!(result, Action::SelectFullPageDown));
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
                Expected::SqlModalInput(c) => {
                    assert!(matches!(result, Action::SqlModalInput(x) if x == c))
                }
                Expected::SqlModalMoveCursor(m) => {
                    assert!(matches!(result, Action::SqlModalMoveCursor(x) if x == m))
                }
                Expected::CloseSqlModal => assert!(matches!(result, Action::CloseSqlModal)),
                Expected::CompletionTrigger => assert!(matches!(result, Action::CompletionTrigger)),
                Expected::CompletionAccept => assert!(matches!(result, Action::CompletionAccept)),
                Expected::CompletionDismiss => assert!(matches!(result, Action::CompletionDismiss)),
                Expected::CompletionPrev => assert!(matches!(result, Action::CompletionPrev)),
                Expected::CompletionNext => assert!(matches!(result, Action::CompletionNext)),
                Expected::None => assert!(matches!(result, Action::None)),
            }
        }

        // Completion-aware keys: behavior when completion is hidden
        #[rstest]
        #[case(Key::Esc, Expected::CloseSqlModal)]
        #[case(Key::Tab, Expected::SqlModalTab)]
        #[case(Key::Enter, Expected::SqlModalNewLine)]
        #[case(Key::Up, Expected::SqlModalMoveCursor(CursorMove::Up))]
        #[case(Key::Down, Expected::SqlModalMoveCursor(CursorMove::Down))]
        fn completion_hidden_key_behavior(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_sql_modal_keys(combo(code), false);

            assert_action(result, expected);
        }

        // Completion-aware keys: behavior when completion is visible
        #[rstest]
        #[case(Key::Esc, Expected::CompletionDismiss)]
        #[case(Key::Tab, Expected::CompletionAccept)]
        #[case(Key::Enter, Expected::CompletionAccept)]
        #[case(Key::Up, Expected::CompletionPrev)]
        #[case(Key::Down, Expected::CompletionNext)]
        fn completion_visible_key_behavior(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_sql_modal_keys(combo(code), true);

            assert_action(result, expected);
        }

        // Keys unaffected by completion visibility
        #[rstest]
        #[case(Key::Backspace, Expected::SqlModalBackspace)]
        #[case(Key::Delete, Expected::SqlModalDelete)]
        #[case(Key::Left, Expected::SqlModalMoveCursor(CursorMove::Left))]
        #[case(Key::Right, Expected::SqlModalMoveCursor(CursorMove::Right))]
        #[case(Key::Home, Expected::SqlModalMoveCursor(CursorMove::Home))]
        #[case(Key::End, Expected::SqlModalMoveCursor(CursorMove::End))]
        #[case(Key::F(1), Expected::None)]
        fn completion_independent_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_sql_modal_keys(combo(code), false);

            assert_action(result, expected);
        }

        #[test]
        fn delete_key_returns_delete_action() {
            let result = handle_sql_modal_keys(combo(Key::Delete), false);

            assert_action(result, Expected::SqlModalDelete);
        }

        #[test]
        fn enter_without_completion_returns_newline() {
            let result = handle_sql_modal_keys(combo(Key::Enter), false);

            assert_action(result, Expected::SqlModalNewLine);
        }

        #[test]
        fn tab_without_completion_returns_tab() {
            let result = handle_sql_modal_keys(combo(Key::Tab), false);

            assert_action(result, Expected::SqlModalTab);
        }

        #[test]
        fn alt_enter_submits_query() {
            let result = handle_sql_modal_keys(combo_alt(Key::Enter), false);

            assert_action(result, Expected::SqlModalSubmit);
        }

        #[test]
        fn ctrl_space_triggers_completion() {
            let result = handle_sql_modal_keys(combo_ctrl(Key::Char(' ')), false);

            assert_action(result, Expected::CompletionTrigger);
        }

        #[rstest]
        #[case('a')]
        #[case('Z')]
        #[case('あ')]
        #[case('日')]
        fn char_input_inserts_character(#[case] c: char) {
            let result = handle_sql_modal_keys(combo(Key::Char(c)), false);

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
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Enter, Expected::Confirm)]
        #[case(Key::Up, Expected::SelectPrev)]
        #[case(Key::Down, Expected::SelectNext)]
        #[case(Key::Backspace, Expected::FilterBackspace)]
        #[case(Key::Char('u'), Expected::FilterInput('u'))]
        #[case(Key::Char('日'), Expected::FilterInput('日'))]
        #[case(Key::Tab, Expected::None)]
        fn table_picker_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_table_picker_keys(combo(code));

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
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Enter, Expected::Confirm)]
        #[case(Key::Up, Expected::SelectPrev)]
        #[case(Key::Down, Expected::SelectNext)]
        #[case(Key::Char('a'), Expected::None)]
        fn command_palette_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_command_palette_keys(combo(code));

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
        fn esc_closes_help() {
            let result = handle_help_keys(combo(Key::Esc));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn question_mark_closes_help() {
            let result = handle_help_keys(combo(Key::Char('?')));

            assert!(matches!(result, Action::CloseHelp));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_help_keys(combo(Key::Char('a')));

            assert!(matches!(result, Action::None));
        }
    }

    mod result_history {
        use super::*;
        use crate::domain::{QueryResult, QuerySource};
        use rstest::rstest;
        use std::sync::Arc;

        fn make_result(query: &str) -> Arc<QueryResult> {
            Arc::new(QueryResult::success(
                query.to_string(),
                vec!["col".to_string()],
                vec![vec!["val".to_string()]],
                10,
                QuerySource::Adhoc,
            ))
        }

        fn state_with_history(count: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            for i in 0..count {
                state
                    .query
                    .result_history
                    .push(make_result(&format!("SELECT {}", i + 1)));
            }
            state.query.current_result = Some(make_result("SELECT latest"));
            state
        }

        #[test]
        fn ctrl_h_opens_result_history() {
            let state = AppState::new("test".to_string());

            let result = handle_normal_mode(combo_ctrl(Key::Char('h')), &state);

            assert!(matches!(result, Action::OpenResultHistory));
        }

        #[test]
        fn bracket_left_navigates_history_older() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(2);

            let result = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(result, Action::HistoryOlder));
        }

        #[test]
        fn bracket_right_navigates_history_newer() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(0);

            let result = handle_normal_mode(combo(Key::Char(']')), &state);

            assert!(matches!(result, Action::HistoryNewer));
        }

        #[test]
        fn ctrl_h_exits_history_when_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);

            let result = handle_normal_mode(combo_ctrl(Key::Char('h')), &state);

            assert!(matches!(result, Action::ExitResultHistory));
        }

        #[test]
        fn help_allowed_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);

            let result = handle_normal_mode(combo(Key::Char('?')), &state);

            assert!(matches!(result, Action::OpenHelp));
        }

        #[rstest]
        #[case(Key::Char('q'), "q (quit)")]
        #[case(Key::Char('s'), "s (sql modal)")]
        #[case(Key::Char('f'), "f (focus toggle)")]
        #[case(Key::Char('r'), "r (reload)")]
        #[case(Key::Char(':'), ": (command line)")]
        #[case(Key::Enter, "Enter")]
        #[case(Key::Esc, "Esc")]
        fn blocked_keys_are_noop_in_history_mode(#[case] key: Key, #[case] label: &str) {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);

            let result = handle_normal_mode(combo(key), &state);

            assert!(
                matches!(result, Action::None),
                "{} should be no-op in history mode, got {:?}",
                label,
                result
            );
        }

        #[test]
        fn scroll_keys_allowed_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);
            state.ui.focus_mode = true;

            assert!(matches!(
                handle_normal_mode(combo(Key::Char('j')), &state),
                Action::ResultScrollDown
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('k')), &state),
                Action::ResultScrollUp
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('h')), &state),
                Action::ResultScrollLeft
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('l')), &state),
                Action::ResultScrollRight
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('g')), &state),
                Action::ResultScrollTop
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('G')), &state),
                Action::ResultScrollBottom
            ));
        }

        #[test]
        fn ctrl_p_and_ctrl_k_blocked_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);

            let p = handle_normal_mode(combo_ctrl(Key::Char('p')), &state);
            let k = handle_normal_mode(combo_ctrl(Key::Char('k')), &state);

            assert!(
                matches!(p, Action::None),
                "^P should be blocked in history mode"
            );
            assert!(
                matches!(k, Action::None),
                "^K should be blocked in history mode"
            );
        }

        #[test]
        fn ctrl_scroll_allowed_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.history_index = Some(1);
            state.ui.focus_mode = true;

            assert!(matches!(
                handle_normal_mode(combo_ctrl(Key::Char('d')), &state),
                Action::ResultScrollHalfPageDown
            ));
            assert!(matches!(
                handle_normal_mode(combo_ctrl(Key::Char('u')), &state),
                Action::ResultScrollHalfPageUp
            ));
        }

        #[test]
        fn bracket_nav_falls_through_when_not_in_history() {
            let mut state = AppState::new("test".to_string());
            state.ui.focus_mode = true;

            let next = handle_normal_mode(combo(Key::Char(']')), &state);
            let prev = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(next, Action::ResultNextPage));
            assert!(matches!(prev, Action::ResultPrevPage));
        }
    }

    mod connection_error {
        use super::*;
        use rstest::rstest;

        enum Expected {
            Close,
            Reenter,
            OpenSelector,
            ToggleDetails,
            Copy,
            ScrollUp,
            ScrollDown,
        }

        #[rstest]
        #[case(Key::Esc, Expected::Close)]
        #[case(Key::Char('e'), Expected::Reenter)]
        #[case(Key::Char('s'), Expected::OpenSelector)]
        #[case(Key::Char('d'), Expected::ToggleDetails)]
        #[case(Key::Char('c'), Expected::Copy)]
        fn connection_error_action_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_connection_error_keys(combo(code));

            match expected {
                Expected::Close => assert!(matches!(result, Action::CloseConnectionError)),
                Expected::Reenter => assert!(matches!(result, Action::ReenterConnectionSetup)),
                Expected::OpenSelector => {
                    assert!(matches!(result, Action::OpenConnectionSelector))
                }
                Expected::ToggleDetails => {
                    assert!(matches!(result, Action::ToggleConnectionErrorDetails))
                }
                Expected::Copy => assert!(matches!(result, Action::CopyConnectionError)),
                _ => unreachable!(),
            }
        }

        #[rstest]
        #[case(Key::Up, Expected::ScrollUp)]
        #[case(Key::Char('k'), Expected::ScrollUp)]
        #[case(Key::Down, Expected::ScrollDown)]
        #[case(Key::Char('j'), Expected::ScrollDown)]
        fn connection_error_scroll_keys(#[case] code: Key, #[case] expected: Expected) {
            let result = handle_connection_error_keys(combo(code));

            match expected {
                Expected::ScrollUp => assert!(matches!(result, Action::ScrollConnectionErrorUp)),
                Expected::ScrollDown => {
                    assert!(matches!(result, Action::ScrollConnectionErrorDown))
                }
                _ => unreachable!(),
            }
        }

        #[test]
        fn connection_error_unbound_keys() {
            let result = handle_connection_error_keys(combo(Key::Tab));

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn r_key_retries_service_connection() {
            let result = handle_connection_error_keys(combo(Key::Char('r')));

            assert!(matches!(result, Action::RetryServiceConnection));
        }
    }

    mod cell_edit_mode {
        use super::*;
        use crate::app::action::CursorMove;

        #[test]
        fn esc_in_cell_edit_returns_cancel_not_discard() {
            let result = handle_cell_edit_keys(combo(Key::Esc));

            assert!(matches!(result, Action::ResultCancelCellEdit));
        }

        #[test]
        fn char_input_returns_cell_edit_input() {
            let result = handle_cell_edit_keys(combo(Key::Char('x')));

            assert!(matches!(result, Action::ResultCellEditInput('x')));
        }

        #[test]
        fn backspace_returns_cell_edit_backspace() {
            let result = handle_cell_edit_keys(combo(Key::Backspace));

            assert!(matches!(result, Action::ResultCellEditBackspace));
        }

        #[test]
        fn delete_returns_cell_edit_delete() {
            let result = handle_cell_edit_keys(combo(Key::Delete));

            assert!(matches!(result, Action::ResultCellEditDelete));
        }

        #[test]
        fn left_returns_move_cursor_left() {
            let result = handle_cell_edit_keys(combo(Key::Left));

            assert!(matches!(
                result,
                Action::ResultCellEditMoveCursor(CursorMove::Left)
            ));
        }

        #[test]
        fn right_returns_move_cursor_right() {
            let result = handle_cell_edit_keys(combo(Key::Right));

            assert!(matches!(
                result,
                Action::ResultCellEditMoveCursor(CursorMove::Right)
            ));
        }

        #[test]
        fn home_returns_move_cursor_home() {
            let result = handle_cell_edit_keys(combo(Key::Home));

            assert!(matches!(
                result,
                Action::ResultCellEditMoveCursor(CursorMove::Home)
            ));
        }

        #[test]
        fn end_returns_move_cursor_end() {
            let result = handle_cell_edit_keys(combo(Key::End));

            assert!(matches!(
                result,
                Action::ResultCellEditMoveCursor(CursorMove::End)
            ));
        }
    }

    /// Smoke tests for mode dispatch: verify handle_key_event routes to correct handler
    mod mode_dispatch {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = mode;
            state
        }

        #[test]
        fn normal_mode_routes_to_normal_handler() {
            let state = make_state(InputMode::Normal);

            // 'q' in Normal mode should quit
            let result = handle_key_event(combo(Key::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn sql_modal_mode_routes_to_sql_modal_handler() {
            let state = make_state(InputMode::SqlModal);

            // Esc in SqlModal should close modal (not Escape action)
            let result = handle_key_event(combo(Key::Esc), &state);

            assert!(matches!(result, Action::CloseSqlModal));
        }
    }

    mod connection_setup_keys {
        use super::*;
        use crate::app::connection_setup_state::ConnectionField;
        use rstest::rstest;

        fn setup_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ConnectionSetup;
            state
        }

        #[test]
        fn tab_moves_to_next_field() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Tab), &state);

            assert!(matches!(result, Action::ConnectionSetupNextField));
        }

        #[test]
        fn backtab_moves_to_prev_field() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::BackTab), &state);

            assert!(matches!(result, Action::ConnectionSetupPrevField));
        }

        #[test]
        fn ctrl_s_saves() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_ctrl(Key::Char('s')), &state);

            assert!(matches!(result, Action::ConnectionSetupSave));
        }

        #[test]
        fn esc_cancels() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Esc), &state);

            assert!(matches!(result, Action::ConnectionSetupCancel));
        }

        #[test]
        fn char_input_sends_input_action() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Char('a')), &state);

            assert!(matches!(result, Action::ConnectionSetupInput('a')));
        }

        #[test]
        fn backspace_sends_backspace_action() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo(Key::Backspace), &state);

            assert!(matches!(result, Action::ConnectionSetupBackspace));
        }

        #[test]
        fn ctrl_c_is_ignored() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_ctrl(Key::Char('c')), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn alt_char_is_allowed_for_international_keyboards() {
            let state = setup_state();

            let result = handle_connection_setup_keys(combo_alt(Key::Char('q')), &state);

            assert!(matches!(result, Action::ConnectionSetupInput('q')));
        }

        #[test]
        fn altgr_char_is_allowed() {
            use crate::app::keybindings::Modifiers;
            let state = setup_state();
            let altgr = KeyCombo {
                key: Key::Char('@'),
                modifiers: Modifiers {
                    ctrl: true,
                    alt: true,
                    shift: false,
                },
            };

            let result = handle_connection_setup_keys(altgr, &state);

            assert!(matches!(result, Action::ConnectionSetupInput('@')));
        }

        #[test]
        fn enter_on_ssl_field_toggles_dropdown() {
            let mut state = setup_state();
            state.connection_setup.focused_field = ConnectionField::SslMode;

            let result = handle_connection_setup_keys(combo(Key::Enter), &state);

            assert!(matches!(result, Action::ConnectionSetupToggleDropdown));
        }

        mod dropdown_open {
            use super::*;

            fn dropdown_state() -> AppState {
                let mut state = setup_state();
                state.connection_setup.ssl_dropdown.is_open = true;
                state
            }

            #[rstest]
            #[case(Key::Up, Action::ConnectionSetupDropdownPrev)]
            #[case(Key::Down, Action::ConnectionSetupDropdownNext)]
            #[case(Key::Enter, Action::ConnectionSetupDropdownConfirm)]
            #[case(Key::Esc, Action::ConnectionSetupDropdownCancel)]
            fn dropdown_navigation(#[case] code: Key, #[case] expected: Action) {
                let state = dropdown_state();

                let result = handle_connection_setup_keys(combo(code), &state);

                assert_eq!(
                    std::mem::discriminant(&result),
                    std::mem::discriminant(&expected)
                );
            }
        }
    }

    mod confirm_dialog_keys {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case(Key::Enter, Action::ConfirmDialogConfirm)]
        #[case(Key::Esc, Action::ConfirmDialogCancel)]
        fn dialog_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_confirm_dialog_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[rstest]
        #[case(Key::Char('y'))]
        #[case(Key::Char('Y'))]
        #[case(Key::Char('n'))]
        #[case(Key::Char('N'))]
        #[case(Key::Char('x'))]
        fn non_bound_keys_return_none(#[case] code: Key) {
            let result = handle_confirm_dialog_keys(combo(code));

            assert!(matches!(result, Action::None));
        }
    }

    mod connection_selector_keys {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case(Key::Char('j'), Action::ConnectionListSelectNext)]
        #[case(Key::Down, Action::ConnectionListSelectNext)]
        #[case(Key::Char('k'), Action::ConnectionListSelectPrevious)]
        #[case(Key::Up, Action::ConnectionListSelectPrevious)]
        fn selector_navigation_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_connection_selector_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[rstest]
        #[case(Key::Enter, Action::ConfirmConnectionSelection)]
        #[case(Key::Char('n'), Action::OpenConnectionSetup)]
        #[case(Key::Char('e'), Action::RequestEditSelectedConnection)]
        #[case(Key::Char('d'), Action::RequestDeleteSelectedConnection)]
        fn selector_action_keys(#[case] code: Key, #[case] expected: Action) {
            let result = handle_connection_selector_keys(combo(code));

            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected)
            );
        }

        #[test]
        fn selector_esc_closes() {
            let result = handle_connection_selector_keys(combo(Key::Esc));

            assert!(matches!(result, Action::Escape));
        }

        #[test]
        fn unknown_key_returns_none() {
            let result = handle_connection_selector_keys(combo(Key::Char('x')));

            assert!(matches!(result, Action::None));
        }
    }

    mod paste_event {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = mode;
            state
        }

        #[test]
        fn paste_event_in_sql_modal_returns_paste_action() {
            let state = make_state(InputMode::SqlModal);

            let result = handle_paste_event("hello".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "hello"));
        }

        #[test]
        fn paste_event_in_table_picker_returns_paste_action() {
            let state = make_state(InputMode::TablePicker);

            let result = handle_paste_event("world".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "world"));
        }

        #[test]
        fn paste_event_in_er_table_picker_returns_paste_action() {
            let state = make_state(InputMode::ErTablePicker);

            let result = handle_paste_event("public.users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "public.users"));
        }

        #[test]
        fn paste_event_in_normal_mode_returns_none() {
            let state = make_state(InputMode::Normal);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn paste_event_in_help_mode_returns_none() {
            let state = make_state(InputMode::Help);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
        }
    }

    mod er_table_picker {
        use super::*;

        #[test]
        fn esc_returns_close_er_table_picker() {
            let result = handle_er_table_picker_keys(combo(Key::Esc));

            assert!(matches!(result, Action::CloseErTablePicker));
        }

        #[test]
        fn enter_returns_er_confirm_selection() {
            let result = handle_er_table_picker_keys(combo(Key::Enter));

            assert!(matches!(result, Action::ErConfirmSelection));
        }

        #[test]
        fn up_returns_select_previous() {
            let result = handle_er_table_picker_keys(combo(Key::Up));

            assert!(matches!(result, Action::SelectPrevious));
        }

        #[test]
        fn down_returns_select_next() {
            let result = handle_er_table_picker_keys(combo(Key::Down));

            assert!(matches!(result, Action::SelectNext));
        }

        #[test]
        fn backspace_returns_er_filter_backspace() {
            let result = handle_er_table_picker_keys(combo(Key::Backspace));

            assert!(matches!(result, Action::ErFilterBackspace));
        }

        #[test]
        fn char_input_returns_er_filter_input() {
            let result = handle_er_table_picker_keys(combo(Key::Char('a')));

            assert!(matches!(result, Action::ErFilterInput('a')));
        }
    }
}
