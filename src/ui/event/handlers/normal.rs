use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::key_sequence::Prefix;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::update::action::Action;
use crate::app::update::input::keybindings::{self as kb, Key, KeyCombo};
use crate::app::update::input::nav_intent::{
    NavIntent, NavigationContext, map_nav_intent, resolve,
};

#[cfg(test)]
use crate::app::model::shared::ui_state::FocusMode;

fn resolve_nav(combo: &KeyCombo, nav_ctx: NavigationContext) -> Option<Action> {
    map_nav_intent(combo).map(|intent| resolve(intent, nav_ctx))
}

pub fn handle_normal_mode(combo: KeyCombo, state: &AppState) -> Action {
    let nav_ctx = NavigationContext::from_state(state);
    let result_navigation = nav_ctx.is_result();
    let inspector_navigation = nav_ctx.is_inspector();
    let result_nav_mode = state.result_interaction.selection().mode();

    // Ctrl combos
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
            _ => {
                if let Some(action) = resolve_nav(&combo, nav_ctx) {
                    return action;
                }
                if state.query.is_history_mode() {
                    return Action::None;
                }
            }
        }
    }

    // Key sequence FSM: two-key sequences (zz, zt, zb)
    // Must be resolved before history whitelist and global actions so that
    // the second key (t, b, z) is never swallowed and the sequence is always cleared.
    if let Some(prefix) = state.ui.key_sequence.pending_prefix() {
        if combo.modifiers.ctrl || combo.modifiers.alt {
            return Action::CancelKeySequence;
        }
        return match prefix {
            Prefix::Z => {
                if inspector_navigation {
                    return Action::CancelKeySequence;
                }
                let intent = match combo.key {
                    Key::Char('z') => NavIntent::ScrollCursorCenter,
                    Key::Char('t') => NavIntent::ScrollCursorTop,
                    Key::Char('b') => NavIntent::ScrollCursorBottom,
                    _ => return Action::CancelKeySequence,
                };
                resolve(intent, nav_ctx)
            }
        };
    }

    // History mode: whitelist — only history nav, help, and scroll allowed
    if state.query.is_history_mode() {
        match combo.key {
            Key::Char('[') => return Action::HistoryOlder,
            Key::Char(']') => return Action::HistoryNewer,
            Key::Char('?') => return Action::OpenHelp,
            // Home/End/PageDown/PageUp are blocked in history mode
            // (only char keys g/G and Ctrl+D/U/F/B are allowed for these motions)
            Key::Home | Key::End | Key::PageDown | Key::PageUp => return Action::None,
            // Scroll keys fall through to normal handling via NavIntent
            _ if map_nav_intent(&combo).is_some() => {}
            Key::Char('z') => {}
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

    // NavIntent-based navigation (context-dependent)
    if let Some(action) = resolve_nav(&combo, nav_ctx) {
        return action;
    }

    // Non-navigation context keys
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

        // Pane switching: exit focus mode first if active
        Key::Char(c @ '1'..='3') => {
            if state.ui.is_focus_mode() {
                Action::ToggleFocus
            } else {
                FocusedPane::from_browse_key(c).map_or(Action::None, Action::SetFocusedPane)
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

        Key::Char('z') => Action::BeginKeySequence(Prefix::Z),

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
    use crate::app::model::shared::key_sequence::KeySequenceState;
    use crate::app::update::action::{
        CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
        SelectMotion,
    };
    use crate::app::update::input::keybindings::{Key, KeyCombo};
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
    fn q_quits() {
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
    fn esc_exits_normal_mode() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Esc), &state);

        assert!(matches!(result, Action::Escape));
    }

    // Navigation keys: equivalent actions
    #[rstest]
    #[case(Key::Up, "up arrow")]
    #[case(Key::Char('k'), "k")]
    fn arrow_up_and_k_select_previous(#[case] code: Key, #[case] _desc: &str) {
        let state = browse_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(result, Action::Select(SelectMotion::Previous)));
    }

    #[rstest]
    #[case(Key::Down, "down arrow")]
    #[case(Key::Char('j'), "j")]
    fn arrow_down_and_j_select_next(#[case] code: Key, #[case] _desc: &str) {
        let state = browse_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(result, Action::Select(SelectMotion::Next)));
    }

    #[rstest]
    #[case(Key::Char('g'), "g")]
    #[case(Key::Home, "home")]
    fn g_and_home_select_first(#[case] code: Key, #[case] _desc: &str) {
        let state = browse_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(result, Action::Select(SelectMotion::First)));
    }

    #[rstest]
    #[case(Key::Char('G'), "capital G")]
    #[case(Key::End, "end")]
    fn g_and_end_select_last(#[case] code: Key, #[case] _desc: &str) {
        let state = browse_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(result, Action::Select(SelectMotion::Last)));
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
    fn number_keys_select_panes(#[case] key_char: char, #[case] expected_pane: FocusedPane) {
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
    fn unknown_key_is_noop() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('x')), &state);

        assert!(matches!(result, Action::None));
    }

    fn focus_mode_state() -> AppState {
        let mut state = browse_state();
        state.ui.focus_mode = FocusMode::focused(FocusedPane::Explorer);
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
    fn focus_mode_j_and_down_scroll_down(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('k'))]
    #[case(Key::Up)]
    fn focus_mode_k_and_up_scroll_up(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('g'))]
    #[case(Key::Home)]
    fn focus_mode_g_and_home_scroll_top(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ToStart
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('G'))]
    #[case(Key::End)]
    fn focus_mode_shift_g_and_end_scroll_bottom(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ToEnd
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('h'))]
    #[case(Key::Left)]
    fn focus_mode_h_and_left_scroll_left(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Left,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('l'))]
    #[case(Key::Right)]
    fn focus_mode_l_and_right_scroll_right(#[case] code: Key) {
        let state = focus_mode_state();
        let result = handle_normal_mode(combo(code), &state);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Right,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn j_scrolls_down() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('j')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn h_scrolls_left() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('h')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Left,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn l_scrolls_right() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('l')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Right,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn row_active_d_sets_delete_pending() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);

        let result = handle_normal_mode(combo(Key::Char('d')), &state);

        assert!(matches!(result, Action::ResultDeleteOperatorPending));
    }

    #[test]
    fn row_active_dd_stages_row_for_delete() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.delete_op_pending = true;

        let result = handle_normal_mode(combo(Key::Char('d')), &state);

        assert!(matches!(result, Action::StageRowForDelete));
    }

    #[test]
    fn scroll_mode_d_is_noop() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('d')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn row_active_y_sets_yank_pending() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultRowYankOperatorPending));
    }

    #[test]
    fn yy_triggers_row_yank() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.yank_op_pending = true;

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultRowYank));
    }

    #[test]
    fn scroll_mode_y_is_noop() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn y_yanks_cell() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(0);

        let result = handle_normal_mode(combo(Key::Char('y')), &state);

        assert!(matches!(result, Action::ResultCellYank));
    }

    #[test]
    fn explorer_h_scrolls_left() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('h')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Explorer,
                direction: ScrollDirection::Left,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn explorer_l_scrolls_right() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('l')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Explorer,
                direction: ScrollDirection::Right,
                amount: ScrollAmount::Line
            }
        ));
    }

    #[test]
    fn ctrl_o_opens_history_picker() {
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
    fn e_opens_er_picker() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('e')), &state);

        assert!(matches!(result, Action::OpenErTablePicker));
    }

    #[test]
    fn cell_active_esc_discards_pending_draft() {
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
    fn cell_active_esc_exits_to_row_active() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1);

        let result = handle_normal_mode(combo(Key::Esc), &state);

        assert!(matches!(result, Action::ResultExitToRowActive));
    }

    #[test]
    fn i_enters_cell_edit() {
        let mut state = result_focused_state();
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1);

        let result = handle_normal_mode(combo(Key::Char('i')), &state);

        assert!(matches!(result, Action::ResultEnterCellEdit));
    }

    #[test]
    fn inactive_cell_i_is_noop() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('i')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn explorer_h_selects_viewport_top() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('H')), &state);

        assert!(matches!(result, Action::Select(SelectMotion::ViewportTop)));
    }

    #[test]
    fn explorer_m_selects_viewport_middle() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('M')), &state);

        assert!(matches!(
            result,
            Action::Select(SelectMotion::ViewportMiddle)
        ));
    }

    #[test]
    fn explorer_l_selects_viewport_bottom() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('L')), &state);

        assert!(matches!(
            result,
            Action::Select(SelectMotion::ViewportBottom)
        ));
    }

    #[test]
    fn h_scrolls_to_viewport_top() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('H')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ViewportTop
            }
        ));
    }

    #[test]
    fn m_scrolls_to_viewport_middle() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('M')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ViewportMiddle
            }
        ));
    }

    #[test]
    fn l_scrolls_to_viewport_bottom() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('L')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ViewportBottom
            }
        ));
    }

    #[test]
    fn focus_mode_m_scrolls_to_viewport_middle() {
        let state = focus_mode_state();

        let result = handle_normal_mode(combo(Key::Char('M')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ViewportMiddle
            }
        ));
    }

    fn inspector_focused_state() -> AppState {
        let mut state = browse_state();
        state.ui.focused_pane = FocusedPane::Inspector;
        state
    }

    #[rstest]
    #[case(Key::Char('g'))]
    #[case(Key::Home)]
    fn inspector_g_and_home_scroll_top(#[case] code: Key) {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Inspector,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ToStart
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('G'))]
    #[case(Key::End)]
    fn inspector_shift_g_and_end_scroll_bottom(#[case] code: Key) {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo(code), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Inspector,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ToEnd
            }
        ));
    }

    #[rstest]
    #[case(Key::Char('H'))]
    #[case(Key::Char('M'))]
    #[case(Key::Char('L'))]
    fn inspector_hml_are_noop(#[case] key: Key) {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo(key), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn c_opens_connection_selector() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::Char('c')), &state);

        assert!(matches!(result, Action::OpenConnectionSelector));
    }

    #[test]
    fn c_is_noop() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('c')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn inspector_c_is_noop() {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo(Key::Char('c')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn bracket_right_moves_next_page() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char(']')), &state);

        assert!(matches!(result, Action::ResultNextPage));
    }

    #[test]
    fn bracket_left_moves_prev_page() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::Char('[')), &state);

        assert!(matches!(result, Action::ResultPrevPage));
    }

    #[test]
    fn explorer_brackets_are_noop() {
        let state = browse_state();

        let right = handle_normal_mode(combo(Key::Char(']')), &state);
        let left = handle_normal_mode(combo(Key::Char('[')), &state);

        assert!(matches!(right, Action::None));
        assert!(matches!(left, Action::None));
    }

    #[test]
    fn focus_mode_bracket_right_moves_next_page() {
        let state = focus_mode_state();

        let result = handle_normal_mode(combo(Key::Char(']')), &state);

        assert!(matches!(result, Action::ResultNextPage));
    }

    #[test]
    fn focus_mode_bracket_left_moves_prev_page() {
        let state = focus_mode_state();

        let result = handle_normal_mode(combo(Key::Char('[')), &state);

        assert!(matches!(result, Action::ResultPrevPage));
    }

    // Page scroll: Ctrl-D/U/F/B and PageDown/PageUp
    #[test]
    fn ctrl_d_scrolls_half_page_down() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage
            }
        ));
    }

    #[test]
    fn ctrl_u_scrolls_half_page_up() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('u')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::HalfPage
            }
        ));
    }

    #[test]
    fn ctrl_f_scrolls_full_page_down() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('f')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage
            }
        ));
    }

    #[test]
    fn ctrl_b_scrolls_full_page_up() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('b')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage
            }
        ));
    }

    #[test]
    fn ctrl_d_scrolls_half_page_down_in_inspector() {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Inspector,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage
            }
        ));
    }

    #[test]
    fn ctrl_d_scrolls_half_page_down_in_explorer() {
        let state = browse_state();

        let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

        assert!(matches!(result, Action::Select(SelectMotion::HalfPageDown)));
    }

    #[test]
    fn pagedown_scrolls_full_page_down() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::PageDown), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage
            }
        ));
    }

    #[test]
    fn pageup_scrolls_full_page_up() {
        let state = result_focused_state();

        let result = handle_normal_mode(combo(Key::PageUp), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage
            }
        ));
    }

    #[test]
    fn pagedown_scrolls_full_page_down_in_inspector() {
        let state = inspector_focused_state();

        let result = handle_normal_mode(combo(Key::PageDown), &state);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::Inspector,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage
            }
        ));
    }

    #[test]
    fn pagedown_scrolls_full_page_down_in_explorer() {
        let state = browse_state();

        let result = handle_normal_mode(combo(Key::PageDown), &state);

        assert!(matches!(result, Action::Select(SelectMotion::FullPageDown)));
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
                    .push_history(make_result(&format!("SELECT {}", i + 1)));
            }
            state.query.set_current_result(make_result("SELECT latest"));
            state
        }

        #[test]
        fn ctrl_h_opens_history() {
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
        fn allows_help() {
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
        fn blocked_keys_are_noop(#[case] key: Key, #[case] label: &str) {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

            let result = handle_normal_mode(combo(key), &state);

            assert!(
                matches!(result, Action::None),
                "{label} should be no-op in history mode, got {result:?}"
            );
        }

        #[test]
        fn allows_scroll_keys() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);
            state.ui.focus_mode = FocusMode::focused(FocusedPane::Explorer);

            assert!(matches!(
                handle_normal_mode(combo(Key::Char('j')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('k')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('h')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Left,
                    amount: ScrollAmount::Line
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('l')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Right,
                    amount: ScrollAmount::Line
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('g')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('G')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('H')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ViewportTop
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('M')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ViewportMiddle
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo(Key::Char('L')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ViewportBottom
                }
            ));
        }

        #[test]
        fn blocks_ctrl_o() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);

            let result = handle_normal_mode(combo_ctrl(Key::Char('o')), &state);

            assert!(
                matches!(result, Action::None),
                "^O should be blocked in history mode"
            );
        }

        #[test]
        fn blocks_ctrl_p_and_ctrl_k() {
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
        fn allows_ctrl_scroll() {
            let mut state = state_with_history(3);
            state.query.enter_history(1);
            state.ui.focus_mode = FocusMode::focused(FocusedPane::Explorer);

            assert!(matches!(
                handle_normal_mode(combo_ctrl(Key::Char('d')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage
                }
            ));
            assert!(matches!(
                handle_normal_mode(combo_ctrl(Key::Char('u')), &state),
                Action::Scroll {
                    target: ScrollTarget::Result,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::HalfPage
                }
            ));
        }

        #[test]
        fn bracket_navigation_falls_through_outside_history() {
            let mut state = AppState::new("test".to_string());
            state.ui.focus_mode = FocusMode::focused(FocusedPane::Explorer);

            let next = handle_normal_mode(combo(Key::Char(']')), &state);
            let prev = handle_normal_mode(combo(Key::Char('[')), &state);

            assert!(matches!(next, Action::ResultNextPage));
            assert!(matches!(prev, Action::ResultPrevPage));
        }
    }

    mod shift_uppercase_regression {
        use super::*;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        use crate::ui::event::key_translator::translate;

        #[test]
        fn shift_g_translates_to_select_last() {
            let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
            let combo = translate(event);
            let state = browse_state();

            let result = handle_normal_mode(combo, &state);

            assert!(matches!(result, Action::Select(SelectMotion::Last)));
        }
    }

    mod key_sequence {
        use super::*;

        #[test]
        fn z_begins_key_sequence() {
            let state = browse_state();

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(result, Action::BeginKeySequence(Prefix::Z)));
        }

        #[test]
        fn zz_scrolls_cursor_center() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Center
                }
            ));
        }

        #[test]
        fn zt_scrolls_cursor_top() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('t')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top
                }
            ));
        }

        #[test]
        fn zb_scrolls_cursor_bottom() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('b')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Bottom
                }
            ));
        }

        #[test]
        fn z_then_unknown_cancels_key_sequence() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('x')), &state);

            assert!(matches!(result, Action::CancelKeySequence));
        }

        #[test]
        fn zz_scrolls_cursor_center_in_result() {
            let mut state = result_focused_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Result,
                    position: CursorPosition::Center
                }
            ));
        }

        #[test]
        fn zt_scrolls_cursor_top_in_result() {
            let mut state = result_focused_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('t')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Result,
                    position: CursorPosition::Top
                }
            ));
        }

        #[test]
        fn zb_scrolls_cursor_bottom_in_result() {
            let mut state = result_focused_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('b')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Result,
                    position: CursorPosition::Bottom
                }
            ));
        }

        #[test]
        fn zz_cancels_key_sequence_in_inspector() {
            let mut state = inspector_focused_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(result, Action::CancelKeySequence));
        }

        #[test]
        fn focus_mode_z_begins_key_sequence() {
            let state = focus_mode_state();

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(result, Action::BeginKeySequence(Prefix::Z)));
        }

        #[test]
        fn focus_mode_zz_scrolls_cursor_center() {
            let mut state = focus_mode_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo(Key::Char('z')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Result,
                    position: CursorPosition::Center
                }
            ));
        }

        #[test]
        fn key_sequence_takes_priority_over_global_actions() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            // '?' is a global action (OpenHelp), but with active key sequence it should
            // cancel the sequence instead of opening help
            let result = handle_normal_mode(combo(Key::Char('?')), &state);

            assert!(matches!(result, Action::CancelKeySequence));
        }

        #[test]
        fn ctrl_modifier_cancels_key_sequence() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(combo_ctrl(Key::Char('t')), &state);

            assert!(matches!(result, Action::CancelKeySequence));
        }

        #[test]
        fn alt_modifier_cancels_key_sequence() {
            let mut state = browse_state();
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

            let result = handle_normal_mode(KeyCombo::alt(Key::Char('b')), &state);

            assert!(matches!(result, Action::CancelKeySequence));
        }

        fn history_mode_state_with_sequence() -> AppState {
            use crate::domain::{QueryResult, QuerySource};
            use std::sync::Arc;

            let mut state = browse_state();
            let qr = Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["col".to_string()],
                vec![vec!["val".to_string()]],
                10,
                QuerySource::Adhoc,
            ));
            state.query.push_history(qr.clone());
            state.query.set_current_result(qr);
            state.query.enter_history(0);
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
            state
        }

        #[test]
        fn zt_scrolls_cursor_top_in_history_mode() {
            let state = history_mode_state_with_sequence();

            let result = handle_normal_mode(combo(Key::Char('t')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Top
                }
            ));
        }

        #[test]
        fn zb_scrolls_cursor_bottom_in_history_mode() {
            let state = history_mode_state_with_sequence();

            let result = handle_normal_mode(combo(Key::Char('b')), &state);

            assert!(matches!(
                result,
                Action::ScrollToCursor {
                    target: ScrollToCursorTarget::Explorer,
                    position: CursorPosition::Bottom
                }
            ));
        }
    }

    mod navigation_matrix {
        use super::*;
        use crate::domain::{QueryResult, QuerySource};
        use std::sync::Arc;

        fn assert_action(actual: Action, expected: Action, ctx: &str, key: &str) {
            assert_eq!(
                format!("{actual:?}"),
                format!("{expected:?}"),
                "[{ctx} + {key}]"
            );
        }

        fn explorer_ctx() -> AppState {
            browse_state()
        }

        fn result_scroll_ctx() -> AppState {
            result_focused_state()
        }

        fn result_row_active_ctx() -> AppState {
            let mut state = result_focused_state();
            state.result_interaction.enter_row(0);
            state
        }

        fn result_cell_active_ctx() -> AppState {
            let mut state = result_focused_state();
            state.result_interaction.enter_row(0);
            state.result_interaction.enter_cell(0);
            state
        }

        fn inspector_ctx() -> AppState {
            inspector_focused_state()
        }

        fn make_result() -> Arc<QueryResult> {
            Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec!["col".to_string()],
                vec![vec!["val".to_string()]],
                10,
                QuerySource::Adhoc,
            ))
        }

        fn history_focus_ctx() -> AppState {
            let mut state = browse_state();
            let qr = make_result();
            state.query.push_history(qr.clone());
            state.query.set_current_result(qr);
            state.query.enter_history(0);
            state.ui.focus_mode = FocusMode::focused(FocusedPane::Explorer);
            state.ui.focused_pane = FocusedPane::Result;
            state
        }

        fn focus_mode_ctx() -> AppState {
            focus_mode_state()
        }

        #[rstest]
        #[case("explorer", Key::Char('j'), Action::Select(SelectMotion::Next))]
        #[case("explorer", Key::Char('k'), Action::Select(SelectMotion::Previous))]
        #[case("result_scroll", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("result_scroll", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("result_row_active", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("result_row_active", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("result_cell_active", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("result_cell_active", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("inspector", Key::Char('j'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("inspector", Key::Char('k'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("history_focus", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("history_focus", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("focus_mode", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("focus_mode", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        fn vertical_navigation_keys_scroll_contextually(
            #[case] ctx_name: &str,
            #[case] key: Key,
            #[case] expected: Action,
        ) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_row_active" => result_row_active_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
                "history_focus" => history_focus_ctx(),
                "focus_mode" => focus_mode_ctx(),
                _ => unreachable!(),
            };
            let key_label = format!("{key:?}");
            let actual = handle_normal_mode(combo(key), &state);
            assert_action(actual, expected, ctx_name, &key_label);
        }

        #[rstest]
        #[case("explorer", Key::Char('g'), Action::Select(SelectMotion::First))]
        #[case("explorer", Key::Char('G'), Action::Select(SelectMotion::Last))]
        #[case("result_scroll", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("result_scroll", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("result_row_active", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("result_row_active", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("result_cell_active", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("result_cell_active", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("inspector", Key::Char('g'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("inspector", Key::Char('G'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("history_focus", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("history_focus", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("focus_mode", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("focus_mode", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        fn first_and_last_navigation_keys_scroll_contextually(
            #[case] ctx_name: &str,
            #[case] key: Key,
            #[case] expected: Action,
        ) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_row_active" => result_row_active_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
                "history_focus" => history_focus_ctx(),
                "focus_mode" => focus_mode_ctx(),
                _ => unreachable!(),
            };
            let key_label = format!("{key:?}");
            let actual = handle_normal_mode(combo(key), &state);
            assert_action(actual, expected, ctx_name, &key_label);
        }

        #[rstest]
        #[case("explorer", Key::Char('H'), Action::Select(SelectMotion::ViewportTop))]
        #[case(
            "explorer",
            Key::Char('M'),
            Action::Select(SelectMotion::ViewportMiddle)
        )]
        #[case(
            "explorer",
            Key::Char('L'),
            Action::Select(SelectMotion::ViewportBottom)
        )]
        #[case("result_scroll", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case("result_scroll", Key::Char('M'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
        #[case("result_scroll", Key::Char('L'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
        #[case("result_row_active", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case(
            "result_row_active",
            Key::Char('M'),
            Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle }
        )]
        #[case(
            "result_row_active",
            Key::Char('L'),
            Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom }
        )]
        #[case("result_cell_active", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case(
            "result_cell_active",
            Key::Char('M'),
            Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle }
        )]
        #[case(
            "result_cell_active",
            Key::Char('L'),
            Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom }
        )]
        #[case("inspector", Key::Char('H'), Action::None)]
        #[case("inspector", Key::Char('M'), Action::None)]
        #[case("inspector", Key::Char('L'), Action::None)]
        #[case("history_focus", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case("history_focus", Key::Char('M'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
        #[case("history_focus", Key::Char('L'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
        #[case("focus_mode", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case("focus_mode", Key::Char('M'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
        #[case("focus_mode", Key::Char('L'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
        fn viewport_navigation_keys_scroll_contextually(
            #[case] ctx_name: &str,
            #[case] key: Key,
            #[case] expected: Action,
        ) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_row_active" => result_row_active_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
                "history_focus" => history_focus_ctx(),
                "focus_mode" => focus_mode_ctx(),
                _ => unreachable!(),
            };
            let key_label = format!("{key:?}");
            let actual = handle_normal_mode(combo(key), &state);
            assert_action(actual, expected, ctx_name, &key_label);
        }

        #[rstest]
        #[case("explorer", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Center })]
        #[case("explorer", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Top })]
        #[case("explorer", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Bottom })]
        #[case("result_scroll", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("result_scroll", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("result_scroll", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        #[case("result_row_active", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("result_row_active", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("result_row_active", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        #[case("result_cell_active", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("result_cell_active", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("result_cell_active", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        #[case("inspector", Key::Char('z'), Action::CancelKeySequence)]
        #[case("inspector", Key::Char('t'), Action::CancelKeySequence)]
        #[case("inspector", Key::Char('b'), Action::CancelKeySequence)]
        #[case("history_focus", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("history_focus", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("history_focus", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        #[case("focus_mode", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("focus_mode", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("focus_mode", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        fn z_prefix_second_keys_scroll_cursor_contextually(
            #[case] ctx_name: &str,
            #[case] key: Key,
            #[case] expected: Action,
        ) {
            let mut state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_row_active" => result_row_active_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
                "history_focus" => history_focus_ctx(),
                "focus_mode" => focus_mode_ctx(),
                _ => unreachable!(),
            };
            state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
            let key_label = format!("{key:?}");
            let actual = handle_normal_mode(combo(key), &state);
            assert_action(actual, expected, ctx_name, &key_label);
        }

        #[rstest]
        #[case("explorer", explorer_ctx())]
        #[case("result_scroll", result_scroll_ctx())]
        #[case("result_row_active", result_row_active_ctx())]
        #[case("result_cell_active", result_cell_active_ctx())]
        #[case("inspector", inspector_ctx())]
        #[case("history_focus", history_focus_ctx())]
        #[case("focus_mode", focus_mode_ctx())]
        fn z_prefix_begins_key_sequence(#[case] ctx_name: &str, #[case] state: AppState) {
            let actual = handle_normal_mode(combo(Key::Char('z')), &state);
            assert_action(actual, Action::BeginKeySequence(Prefix::Z), ctx_name, "z");
        }

        mod history_pane_edges {
            use super::*;

            fn history_explorer_ctx() -> AppState {
                let mut state = history_focus_ctx();
                state.ui.focused_pane = FocusedPane::Explorer;
                state.ui.focus_mode = FocusMode::Normal;
                state
            }

            fn history_inspector_ctx() -> AppState {
                let mut state = history_focus_ctx();
                state.ui.focused_pane = FocusedPane::Inspector;
                state.ui.focus_mode = FocusMode::Normal;
                state
            }

            #[test]
            fn j_selects_next() {
                let state = history_explorer_ctx();
                let actual = handle_normal_mode(combo(Key::Char('j')), &state);
                assert_action(
                    actual,
                    Action::Select(SelectMotion::Next),
                    "history+explorer",
                    "j",
                );
            }

            #[test]
            fn h_selects_viewport_top() {
                let state = history_explorer_ctx();
                let actual = handle_normal_mode(combo(Key::Char('H')), &state);
                assert_action(
                    actual,
                    Action::Select(SelectMotion::ViewportTop),
                    "history+explorer",
                    "H",
                );
            }

            #[test]
            fn j_scrolls_down() {
                let state = history_inspector_ctx();
                let actual = handle_normal_mode(combo(Key::Char('j')), &state);
                assert_action(
                    actual,
                    Action::Scroll {
                        target: ScrollTarget::Inspector,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    "history+inspector",
                    "j",
                );
            }

            #[test]
            fn h_is_noop() {
                let state = history_inspector_ctx();
                let actual = handle_normal_mode(combo(Key::Char('H')), &state);
                assert_action(actual, Action::None, "history+inspector", "H");
            }

            #[test]
            fn zz_scrolls_cursor() {
                let mut state = history_explorer_ctx();
                state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
                let actual = handle_normal_mode(combo(Key::Char('z')), &state);
                assert_action(
                    actual,
                    Action::ScrollToCursor {
                        target: ScrollToCursorTarget::Explorer,
                        position: CursorPosition::Center,
                    },
                    "history+explorer+key_sequence",
                    "z",
                );
            }

            #[test]
            fn history_zz_inspector_clears() {
                let mut state = history_inspector_ctx();
                state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
                let actual = handle_normal_mode(combo(Key::Char('z')), &state);
                assert_action(
                    actual,
                    Action::CancelKeySequence,
                    "history+inspector+key_sequence",
                    "z",
                );
            }
        }

        mod history_whitelist_asymmetry {
            use super::*;

            fn history_result_ctx() -> AppState {
                history_focus_ctx()
            }

            #[test]
            fn home_blocked_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::Home), &state);
                assert_action(actual, Action::None, "history+result", "Home");
            }

            #[test]
            fn end_blocked_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::End), &state);
                assert_action(actual, Action::None, "history+result", "End");
            }

            #[test]
            fn pagedown_blocked_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::PageDown), &state);
                assert_action(actual, Action::None, "history+result", "PageDown");
            }

            #[test]
            fn pageup_blocked_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::PageUp), &state);
                assert_action(actual, Action::None, "history+result", "PageUp");
            }

            #[test]
            fn up_allowed_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::Up), &state);
                assert_action(
                    actual,
                    Action::Scroll {
                        target: ScrollTarget::Result,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    "history+result",
                    "Up",
                );
            }

            #[test]
            fn down_allowed_in_history() {
                let state = history_result_ctx();
                let actual = handle_normal_mode(combo(Key::Down), &state);
                assert_action(
                    actual,
                    Action::Scroll {
                        target: ScrollTarget::Result,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    "history+result",
                    "Down",
                );
            }
        }
    }
}
