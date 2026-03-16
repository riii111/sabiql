use crate::app::action::Action;
use crate::app::focused_pane::FocusedPane;
use crate::app::inspector_tab::InspectorTab;
use crate::app::keybindings::{self as kb, Key, KeyCombo};
use crate::app::state::AppState;
use crate::app::ui_state::ResultNavMode;

pub fn handle_normal_mode(combo: KeyCombo, state: &AppState) -> Action {
    let result_navigation = state.ui.focus_mode || state.ui.focused_pane == FocusedPane::Result;
    let inspector_navigation = state.ui.focused_pane == FocusedPane::Inspector;
    let result_nav_mode = state.result_interaction.selection().mode();

    // Ctrl combos (context-independent)
    if combo.modifiers.ctrl {
        match combo.key {
            Key::Char('p') if !state.query.is_history_mode() => {
                return Action::OpenTablePicker;
            }
            Key::Char('h') => {
                return if state.query.is_history_mode() {
                    Action::ExitResultHistory
                } else {
                    Action::OpenResultHistory
                };
            }
            Key::Char('k') if !state.query.is_history_mode() => {
                return Action::OpenCommandPalette;
            }
            Key::Char('r') => {
                return Action::ToggleReadOnly;
            }
            Key::Char('o') if !state.query.is_history_mode() => {
                return Action::OpenQueryHistoryPicker;
            }
            Key::Char('e') if state.query.visible_result().is_some_and(|r| !r.is_error()) => {
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
                if state.query.is_history_mode() {
                    return Action::None;
                }
            }
        }
    }

    // History mode: whitelist — only history nav, help, and scroll allowed
    if state.query.is_history_mode() {
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
                        if state.result_interaction.cell_edit().has_pending_draft() {
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

        Key::Char('y') if result_navigation && result_nav_mode == ResultNavMode::RowActive => {
            if state.result_interaction.yank_op_pending {
                Action::ResultRowYank
            } else {
                Action::ResultRowYankOperatorPending
            }
        }
        Key::Char('y') if result_navigation && result_nav_mode == ResultNavMode::CellActive => {
            Action::ResultCellYank
        }
        Key::Char('y') if inspector_navigation && state.ui.inspector_tab == InspectorTab::Ddl => {
            Action::DdlYank
        }
        Key::Char('d') if result_navigation && result_nav_mode == ResultNavMode::RowActive => {
            if state.result_interaction.delete_op_pending {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

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
        state.result_interaction.enter_row(0);

        let result = handle_normal_mode(combo(Key::Char('d')), &state);

        assert!(matches!(result, Action::ResultDeleteOperatorPending));
    }

    #[test]
    fn dd_stages_row_for_delete_in_row_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.delete_op_pending = true;

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
    fn y_sets_yank_op_pending_in_row_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultRowYankOperatorPending));
    }

    #[test]
    fn yy_triggers_row_yank_in_row_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.yank_op_pending = true;

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultRowYank));
    }

    #[test]
    fn y_in_scroll_mode_ignored() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn y_in_cell_active_still_cell_yank() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(0);

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultCellYank));
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
    fn ctrl_o_opens_query_history_picker() {
        let state = browse_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('o')), &state);

        assert!(matches!(result, Action::OpenQueryHistoryPicker));
    }

    #[test]
    fn ctrl_r_toggles_read_only() {
        let state = browse_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('r')), &state);

        assert!(matches!(result, Action::ToggleReadOnly));
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
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1);
        state
            .result_interaction
            .begin_cell_edit(0, 1, "original".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("modified".to_string());

        let result = handle_normal_mode(combo(Key::Esc), &state);

        assert!(matches!(result, Action::ResultDiscardCellEdit));
    }

    #[test]
    fn esc_in_cell_active_without_draft_returns_exit_to_row_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1);

        let result = handle_normal_mode(combo(Key::Esc), &state);

        assert!(matches!(result, Action::ResultExitToRowActive));
    }

    #[test]
    fn i_key_enters_cell_edit_when_cell_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1);

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

    mod result_history {
        use super::*;
        use crate::domain::{QueryResult, QuerySource};
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
            state.query.set_current_result(make_result("SELECT latest"));
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
            state.query.enter_history(2);

            let result = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(result, Action::HistoryOlder));
        }

        #[test]
        fn bracket_right_navigates_history_newer() {
            let mut state = state_with_history(3);
            state.query.enter_history(0);

            let result = handle_normal_mode(combo(Key::Char(']')), &state);

            assert!(matches!(result, Action::HistoryNewer));
        }

        #[test]
        fn ctrl_h_exits_history_when_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

            let result = handle_normal_mode(combo_ctrl(Key::Char('h')), &state);

            assert!(matches!(result, Action::ExitResultHistory));
        }

        #[test]
        fn help_allowed_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

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
            state.query.enter_history(1);

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
            state.query.enter_history(1);
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
        fn ctrl_o_blocked_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

            let result = handle_normal_mode(combo_ctrl(Key::Char('o')), &state);

            assert!(
                matches!(result, Action::None),
                "^O should be blocked in history mode"
            );
        }

        #[test]
        fn ctrl_p_and_ctrl_k_blocked_in_history_mode() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

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
            state.query.enter_history(1);
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
}
