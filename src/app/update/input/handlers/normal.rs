use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::key_sequence::Prefix;
use crate::update::action::{Action, ModalKind};
use crate::update::input::keybindings::{self as kb, Key, KeyCombo, Modifiers};
use crate::update::input::vim::{
    BrowseVimContext, VimSurfaceContext, action_for_input, action_for_key,
};

pub fn handle_normal_mode(combo: KeyCombo, state: &AppState) -> Action {
    let browse_ctx = BrowseVimContext::from(state);
    let result_navigation = browse_ctx.is_result();
    let inspector_navigation = browse_ctx.is_inspector();

    // Key sequence FSM: two-key sequences (zz, zt, zb)
    // Must be resolved before Ctrl/global actions so that the second key is
    // never swallowed and the sequence is always cleared.
    if let Some(prefix) = state.ui.key_sequence.pending_prefix() {
        if combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) {
            return Action::CancelKeySequence;
        }
        return match action_for_input(&combo, Some(prefix), VimSurfaceContext::Browse(browse_ctx)) {
            Some(Action::None) | None => Action::CancelKeySequence,
            Some(action) => action,
        };
    }

    // Ctrl combos
    if combo.modifiers.contains(Modifiers::CTRL) {
        match combo.key {
            Key::Char('k') => {
                return Action::OpenModal(ModalKind::Settings);
            }
            Key::Char('r') => {
                return Action::ToggleReadOnly;
            }
            Key::Char('o') => {
                return Action::OpenModal(ModalKind::QueryHistoryPicker);
            }
            Key::Char('e') if state.can_request_csv_export() => {
                return Action::RequestCsvExport;
            }
            Key::Char('p') => {
                return Action::OpenModal(ModalKind::TablePicker);
            }
            // Ctrl+N navigation disabled on main screen; use j/k or arrows.
            // Modals/pickers handle Ctrl+N via their own bindings.
            // NOTE: vim/classify.rs still maps Ctrl+N → MoveDown for modal
            // contexts (SQL Modal Plan/Compare). This catch-all prevents that
            // mapping from reaching the main screen.
            Key::Char('n') => {
                return Action::None;
            }
            _ => {
                if let Some(action) = action_for_key(&combo, VimSurfaceContext::Browse(browse_ctx))
                {
                    return action;
                }
            }
        }
    }

    // Global actions (predicate-based, no modifiers)
    if kb::is_quit(&combo) {
        return Action::Quit;
    }
    if kb::is_help(&combo) {
        return Action::ToggleModal(ModalKind::Help);
    }
    if kb::is_command_line(&combo) {
        return Action::EnterCommandLine;
    }
    if kb::is_command_palette(&combo) {
        return Action::OpenModal(ModalKind::CommandPalette);
    }
    if kb::is_reload(&combo) {
        return Action::ReloadMetadata;
    }
    if kb::is_focus_toggle(&combo) {
        return Action::ToggleFocus;
    }
    if combo.key == Key::Enter
        && combo.modifiers.is_empty()
        && state.connection_error.error_info.is_some()
    {
        return Action::ConfirmSelection;
    }

    // Shared vim semantics (navigation, mode, operators)
    if let Some(action) = action_for_key(&combo, VimSurfaceContext::Browse(browse_ctx)) {
        return action;
    }

    // Non-navigation context keys
    match combo.key {
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

        Key::Char('u')
            if result_navigation && !state.result_interaction.staged_delete_rows().is_empty() =>
        {
            Action::UnstageLastStagedRow
        }
        Key::Char('Y')
            if result_navigation && state.result_interaction.selection().cell().is_some() =>
        {
            Action::ResultCellYank
        }
        Key::Char('s') => Action::OpenModal(ModalKind::SqlModal),
        Key::Char('e') => Action::OpenModal(ModalKind::ErTablePicker),
        Key::Char('c') if state.ui.focused_pane == FocusedPane::Explorer => {
            Action::OpenModal(ModalKind::ConnectionSelector)
        }

        Key::Char('z') => Action::BeginKeySequence(Prefix::Z),

        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::connection::error::ConnectionErrorInfo;
    use crate::model::shared::key_sequence::KeySequenceState;
    use crate::model::shared::ui_state::FocusMode;
    use crate::update::action::{
        CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
        SelectMotion,
    };
    use crate::update::input::keybindings::{Key, KeyCombo};
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

    fn inspector_focused_state() -> AppState {
        let mut state = browse_state();
        state.ui.focused_pane = FocusedPane::Inspector;
        state
    }

    mod dispatch_stage {
        use super::*;
        use rstest::rstest;

        mod global_actions {
            use super::*;

            #[test]
            fn ctrl_p_opens_table_picker() {
                let state = browse_state();

                let result = handle_normal_mode(combo_ctrl(Key::Char('p')), &state);

                assert!(matches!(result, Action::OpenModal(ModalKind::TablePicker)));
            }

            #[test]
            fn plain_p_is_noop() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::Char('p')), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn ctrl_k_opens_settings() {
                let state = browse_state();

                let result = handle_normal_mode(combo_ctrl(Key::Char('k')), &state);

                assert!(matches!(result, Action::OpenModal(ModalKind::Settings)));
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

                assert!(matches!(result, Action::ToggleModal(ModalKind::Help)));
            }

            #[test]
            fn colon_enters_command_line() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::Char(':')), &state);

                assert!(matches!(result, Action::EnterCommandLine));
            }

            #[test]
            fn f1_opens_command_palette() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::F(1)), &state);

                assert!(matches!(
                    result,
                    Action::OpenModal(ModalKind::CommandPalette)
                ));
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

            #[test]
            fn unknown_key_returns_none() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::Char('x')), &state);

                assert!(matches!(result, Action::None));
            }
        }

        mod navigation_aliases {
            use super::*;

            #[rstest]
            #[case(Key::Up)]
            #[case(Key::Char('k'))]
            fn navigation_selects_previous(#[case] code: Key) {
                let state = browse_state();

                let result = handle_normal_mode(combo(code), &state);

                assert!(matches!(result, Action::Select(SelectMotion::Previous)));
            }

            #[rstest]
            #[case(Key::Down)]
            #[case(Key::Char('j'))]
            fn navigation_selects_next(#[case] code: Key) {
                let state = browse_state();

                let result = handle_normal_mode(combo(code), &state);

                assert!(matches!(result, Action::Select(SelectMotion::Next)));
            }

            #[test]
            fn ctrl_n_is_noop_on_main_screen() {
                let state = browse_state();

                let result = handle_normal_mode(combo_ctrl(Key::Char('n')), &state);

                assert!(matches!(result, Action::None));
            }

            #[rstest]
            #[case(Key::Char('g'))]
            #[case(Key::Home)]
            fn navigation_selects_first(#[case] code: Key) {
                let state = browse_state();

                let result = handle_normal_mode(combo(code), &state);

                assert!(matches!(result, Action::Select(SelectMotion::First)));
            }

            #[rstest]
            #[case(Key::Char('G'))]
            #[case(Key::End)]
            fn navigation_selects_last(#[case] code: Key) {
                let state = browse_state();

                let result = handle_normal_mode(combo(code), &state);

                assert!(matches!(result, Action::Select(SelectMotion::Last)));
            }
        }

        mod enter_behavior {
            use super::*;

            #[test]
            fn enter_confirms_selection_when_explorer_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Explorer;

                let result = handle_normal_mode(combo(Key::Enter), &state);

                assert!(matches!(result, Action::ConfirmSelection));
            }

            #[test]
            fn alt_enter_noop_when_connection_error_is_open() {
                let mut state = browse_state();
                state.connection_error.error_info = Some(ConnectionErrorInfo::new("boom"));

                let result = handle_normal_mode(KeyCombo::alt(Key::Enter), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn plain_enter_confirms_connection_error() {
                let mut state = browse_state();
                state.connection_error.error_info = Some(ConnectionErrorInfo::new("boom"));

                let result = handle_normal_mode(KeyCombo::plain(Key::Enter), &state);

                assert!(matches!(result, Action::ConfirmSelection));
            }

            #[test]
            fn enter_noop_when_inspector_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Inspector;

                let result = handle_normal_mode(combo(Key::Enter), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn enter_activates_cell_when_result_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Result;

                let result = handle_normal_mode(combo(Key::Enter), &state);

                assert!(matches!(result, Action::ResultActivateCell));
            }
        }

        mod pane_switch_and_tabs {
            use super::*;

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
            fn shift_tab_prev_when_inspector_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Inspector;

                let result = handle_normal_mode(combo(Key::BackTab), &state);

                assert!(matches!(result, Action::InspectorPrevTab));
            }

            #[test]
            fn tab_noop_when_explorer_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Explorer;

                let result = handle_normal_mode(combo(Key::Tab), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn tab_noop_when_result_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Result;

                let result = handle_normal_mode(combo(Key::Tab), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn backtab_noop_when_explorer_focused() {
                let mut state = browse_state();
                state.ui.focused_pane = FocusedPane::Explorer;

                let result = handle_normal_mode(combo(Key::BackTab), &state);

                assert!(matches!(result, Action::None));
            }
        }
    }

    mod pane_contracts {
        use super::*;
        use rstest::rstest;

        mod explorer_navigation {
            use super::*;

            #[test]
            fn h_scrolls_left() {
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
            fn l_scrolls_right() {
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

            #[rstest]
            #[case(Key::Char('H'), SelectMotion::ViewportTop)]
            #[case(Key::Char('M'), SelectMotion::ViewportMiddle)]
            #[case(Key::Char('L'), SelectMotion::ViewportBottom)]
            fn hml_selects_viewport(#[case] key: Key, #[case] motion: SelectMotion) {
                let state = browse_state();

                let result = handle_normal_mode(combo(key), &state);

                assert!(matches!(result, Action::Select(actual_motion) if actual_motion == motion));
            }

            #[test]
            fn c_opens_connection_selector() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::Char('c')), &state);

                assert!(matches!(
                    result,
                    Action::OpenModal(ModalKind::ConnectionSelector)
                ));
            }
        }

        mod inspector_navigation {
            use super::*;

            #[rstest]
            #[case(Key::Char('g'))]
            #[case(Key::Home)]
            fn g_scrolls_top(#[case] code: Key) {
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
            fn shift_g_scrolls_bottom(#[case] code: Key) {
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

            #[test]
            fn ctrl_p_opens_table_picker_from_inspector() {
                let state = inspector_focused_state();

                let result = handle_normal_mode(combo_ctrl(Key::Char('p')), &state);

                assert!(matches!(result, Action::OpenModal(ModalKind::TablePicker)));
            }

            #[rstest]
            #[case(Key::Char('H'))]
            #[case(Key::Char('M'))]
            #[case(Key::Char('L'))]
            fn hml_noop(#[case] key: Key) {
                let state = inspector_focused_state();

                let result = handle_normal_mode(combo(key), &state);

                assert!(matches!(result, Action::None));
            }

            #[test]
            fn c_noop() {
                let state = inspector_focused_state();

                let result = handle_normal_mode(combo(Key::Char('c')), &state);

                assert!(matches!(result, Action::None));
            }
        }

        mod result_scroll {
            use super::*;

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

            #[rstest]
            #[case(Key::Char('H'), ScrollDirection::Up, ScrollAmount::ViewportTop)]
            #[case(Key::Char('M'), ScrollDirection::Up, ScrollAmount::ViewportMiddle)]
            #[case(Key::Char('L'), ScrollDirection::Down, ScrollAmount::ViewportBottom)]
            fn hml_scrolls_to_viewport(
                #[case] key: Key,
                #[case] direction: ScrollDirection,
                #[case] amount: ScrollAmount,
            ) {
                let state = result_focused_state();

                let result = handle_normal_mode(combo(key), &state);

                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::Result,
                        direction: actual_direction,
                        amount: actual_amount
                    } if actual_direction == direction && actual_amount == amount
                ));
            }

            #[test]
            fn bracket_right_returns_next_page() {
                let state = result_focused_state();

                let result = handle_normal_mode(combo(Key::Char(']')), &state);

                assert!(matches!(result, Action::ResultNextPage));
            }

            #[test]
            fn bracket_left_returns_prev_page() {
                let state = result_focused_state();

                let result = handle_normal_mode(combo(Key::Char('[')), &state);

                assert!(matches!(result, Action::ResultPrevPage));
            }

            #[test]
            fn c_noop() {
                let state = result_focused_state();

                let result = handle_normal_mode(combo(Key::Char('c')), &state);

                assert!(matches!(result, Action::None));
            }
        }

        mod page_scroll {
            use super::*;

            #[test]
            fn ctrl_d_result_half_page_down() {
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
            fn ctrl_u_result_half_page_up() {
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

            #[rstest]
            #[case(combo_ctrl(Key::Char('f')))]
            #[case(combo(Key::PageDown))]
            fn full_page_down_result(#[case] input: KeyCombo) {
                let state = result_focused_state();

                let result = handle_normal_mode(input, &state);

                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::Result,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::FullPage
                    }
                ));
            }

            #[rstest]
            #[case(combo_ctrl(Key::Char('b')))]
            #[case(combo(Key::PageUp))]
            fn full_page_up_result(#[case] input: KeyCombo) {
                let state = result_focused_state();

                let result = handle_normal_mode(input, &state);

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
            fn ctrl_d_inspector_half_page_down() {
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
            fn page_down_inspector_full_page_down() {
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
            fn ctrl_d_explorer_half_page_down() {
                let state = browse_state();

                let result = handle_normal_mode(combo_ctrl(Key::Char('d')), &state);

                assert!(matches!(result, Action::Select(SelectMotion::HalfPageDown)));
            }

            #[test]
            fn page_down_explorer_full_page_down() {
                let state = browse_state();

                let result = handle_normal_mode(combo(Key::PageDown), &state);

                assert!(matches!(result, Action::Select(SelectMotion::FullPageDown)));
            }
        }
    }

    mod stateful_modes {
        use super::*;

        mod focus_mode {
            use super::*;

            #[rstest]
            #[case(Key::Char('j'))]
            #[case(Key::Down)]
            fn j_scrolls_down(#[case] code: Key) {
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
            fn k_scrolls_up(#[case] code: Key) {
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
            fn g_scrolls_top(#[case] code: Key) {
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
            fn shift_g_scrolls_bottom(#[case] code: Key) {
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
            fn h_scrolls_left(#[case] code: Key) {
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
            fn l_scrolls_right(#[case] code: Key) {
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
            fn m_scrolls_to_viewport_middle() {
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

            #[test]
            fn bracket_right_returns_next_page() {
                let state = focus_mode_state();

                let result = handle_normal_mode(combo(Key::Char(']')), &state);

                assert!(matches!(result, Action::ResultNextPage));
            }

            #[test]
            fn bracket_left_returns_prev_page() {
                let state = focus_mode_state();

                let result = handle_normal_mode(combo(Key::Char('[')), &state);

                assert!(matches!(result, Action::ResultPrevPage));
            }
        }

        mod result_cell_active {
            use super::*;

            fn active_cell_state() -> AppState {
                let mut state = result_focused_state();
                state.result_interaction.activate_cell(0, 0);
                state
            }

            #[test]
            fn d_sets_delete_op_pending() {
                let state = active_cell_state();

                let result = handle_normal_mode(combo(Key::Char('d')), &state);

                assert!(matches!(result, Action::ResultDeleteOperatorPending));
            }

            #[test]
            fn dd_stages_row_for_delete() {
                let mut state = active_cell_state();
                state.result_interaction.start_delete_operator();

                let result = handle_normal_mode(combo(Key::Char('d')), &state);

                assert!(matches!(result, Action::StageRowForDelete));
            }

            #[test]
            fn y_sets_row_yank_pending() {
                let state = active_cell_state();

                let result = handle_normal_mode(combo(Key::Char('y')), &state);

                assert!(matches!(result, Action::ResultRowYankOperatorPending));
            }

            #[test]
            fn yy_triggers_row_yank() {
                let mut state = active_cell_state();
                state.result_interaction.start_yank_operator();

                let result = handle_normal_mode(combo(Key::Char('y')), &state);

                assert!(matches!(result, Action::ResultRowYank));
            }

            #[test]
            fn capital_y_yanks_cell() {
                let state = active_cell_state();

                let result = handle_normal_mode(combo(Key::Char('Y')), &state);

                assert!(matches!(result, Action::ResultCellYank));
            }

            #[test]
            fn esc_with_draft_discards_edit() {
                let mut state = result_focused_state();
                state.result_interaction.activate_cell(0, 1);
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
            fn esc_without_draft_exits_to_scroll() {
                let mut state = result_focused_state();
                state.result_interaction.activate_cell(0, 1);

                let result = handle_normal_mode(combo(Key::Esc), &state);

                assert!(matches!(result, Action::ResultExitToScroll));
            }

            #[test]
            fn i_enters_cell_edit() {
                let mut state = result_focused_state();
                state.result_interaction.activate_cell(0, 1);

                let result = handle_normal_mode(combo(Key::Char('i')), &state);

                assert!(matches!(result, Action::ResultEnterCellEdit));
            }

            #[rstest]
            #[case(Key::Char('d'))]
            #[case(Key::Char('y'))]
            #[case(Key::Char('i'))]
            fn operator_noop_without_active_cell(#[case] key: Key) {
                let state = result_focused_state();

                let result = handle_normal_mode(combo(key), &state);

                assert!(matches!(result, Action::None));
            }
        }

        mod key_sequence {
            use super::*;

            mod begin {
                use super::*;

                #[test]
                fn z_starts_sequence_in_browse_mode() {
                    let state = browse_state();

                    let result = handle_normal_mode(combo(Key::Char('z')), &state);

                    assert!(matches!(result, Action::BeginKeySequence(Prefix::Z)));
                }

                #[test]
                fn z_starts_sequence_in_focus_mode() {
                    let state = focus_mode_state();

                    let result = handle_normal_mode(combo(Key::Char('z')), &state);

                    assert!(matches!(result, Action::BeginKeySequence(Prefix::Z)));
                }
            }

            mod explorer {
                use super::*;

                #[rstest]
                #[case(Key::Char('z'), CursorPosition::Center)]
                #[case(Key::Char('t'), CursorPosition::Top)]
                #[case(Key::Char('b'), CursorPosition::Bottom)]
                fn z_prefix_scrolls_cursor(#[case] key: Key, #[case] position: CursorPosition) {
                    let mut state = browse_state();
                    state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

                    let result = handle_normal_mode(combo(key), &state);

                    assert!(matches!(
                        result,
                        Action::ScrollToCursor {
                            target: ScrollToCursorTarget::Explorer,
                            position: actual_position
                        } if actual_position == position
                    ));
                }
            }

            mod result {
                use super::*;

                #[rstest]
                #[case(Key::Char('z'), CursorPosition::Center)]
                #[case(Key::Char('t'), CursorPosition::Top)]
                #[case(Key::Char('b'), CursorPosition::Bottom)]
                fn z_prefix_scrolls_cursor(#[case] key: Key, #[case] position: CursorPosition) {
                    let mut state = result_focused_state();
                    state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

                    let result = handle_normal_mode(combo(key), &state);

                    assert!(matches!(
                        result,
                        Action::ScrollToCursor {
                            target: ScrollToCursorTarget::Result,
                            position: actual_position
                        } if actual_position == position
                    ));
                }
            }

            mod inspector {
                use super::*;

                #[test]
                fn zz_cancels_sequence() {
                    let mut state = inspector_focused_state();
                    state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);

                    let result = handle_normal_mode(combo(Key::Char('z')), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }
            }

            mod focus_mode {
                use super::*;

                #[test]
                fn zz_scrolls_cursor_to_center() {
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
            }

            mod cancel_and_precedence {
                use super::*;

                fn state_waiting_z_prefix() -> AppState {
                    let mut state = browse_state();
                    state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
                    state
                }

                fn state_waiting_z_prefix_with_result() -> AppState {
                    use std::sync::Arc;

                    use crate::domain::{QueryResult, QuerySource};

                    let mut state = state_waiting_z_prefix();
                    state
                        .query
                        .set_current_result(Arc::new(QueryResult::success(
                            "SELECT 1".to_string(),
                            vec!["col".to_string()],
                            vec![vec!["value".to_string()]],
                            1,
                            QuerySource::Preview,
                        )));
                    state
                }

                #[test]
                fn unknown_key_cancels_sequence() {
                    let state = state_waiting_z_prefix();

                    let result = handle_normal_mode(combo(Key::Char('x')), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }

                #[test]
                fn takes_priority_over_global_actions() {
                    let state = state_waiting_z_prefix();

                    let result = handle_normal_mode(combo(Key::Char('?')), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }

                #[rstest]
                #[case(Key::Char('k'))]
                #[case(Key::Char('o'))]
                #[case(Key::Char('p'))]
                #[case(Key::Char('r'))]
                #[case(Key::Char('n'))]
                #[case(Key::Char('t'))]
                fn ctrl_modifier_cancels_sequence(#[case] key: Key) {
                    let state = state_waiting_z_prefix();

                    let result = handle_normal_mode(combo_ctrl(key), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }

                #[test]
                fn ctrl_e_cancels_sequence_even_when_export_is_available() {
                    let state = state_waiting_z_prefix_with_result();

                    let result = handle_normal_mode(combo_ctrl(Key::Char('e')), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }

                #[test]
                fn alt_modifier_cancels_sequence() {
                    let state = state_waiting_z_prefix();

                    let result = handle_normal_mode(KeyCombo::alt(Key::Char('b')), &state);

                    assert!(matches!(result, Action::CancelKeySequence));
                }
            }
        }
    }

    mod navigation_matrix {
        use super::*;

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

        fn result_cell_active_ctx() -> AppState {
            let mut state = result_focused_state();
            state.result_interaction.activate_cell(0, 0);
            state
        }

        fn inspector_ctx() -> AppState {
            inspector_focused_state()
        }

        fn focus_mode_ctx() -> AppState {
            focus_mode_state()
        }

        #[test]
        fn shift_g_event_translates_to_select_last() {
            let combo = KeyCombo::plain(Key::Char('G'));
            let state = explorer_ctx();

            let result = handle_normal_mode(combo, &state);

            assert!(matches!(result, Action::Select(SelectMotion::Last)));
        }

        #[rstest]
        #[case("explorer", Key::Char('j'), Action::Select(SelectMotion::Next))]
        #[case("explorer", Key::Char('k'), Action::Select(SelectMotion::Previous))]
        #[case("result_scroll", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("result_scroll", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("result_cell_active", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("result_cell_active", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("inspector", Key::Char('j'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("inspector", Key::Char('k'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        #[case("focus_mode", Key::Char('j'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
        #[case("focus_mode", Key::Char('k'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
        fn vertical_jk(#[case] ctx_name: &str, #[case] key: Key, #[case] expected: Action) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
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
        #[case("result_cell_active", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("result_cell_active", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("inspector", Key::Char('g'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("inspector", Key::Char('G'), Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        #[case("focus_mode", Key::Char('g'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
        #[case("focus_mode", Key::Char('G'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
        fn ends_g_shift_g(#[case] ctx_name: &str, #[case] key: Key, #[case] expected: Action) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
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
        #[case("focus_mode", Key::Char('H'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
        #[case("focus_mode", Key::Char('M'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
        #[case("focus_mode", Key::Char('L'), Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
        fn viewport_hml(#[case] ctx_name: &str, #[case] key: Key, #[case] expected: Action) {
            let state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
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
        #[case("result_cell_active", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("result_cell_active", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("result_cell_active", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        #[case("inspector", Key::Char('z'), Action::CancelKeySequence)]
        #[case("inspector", Key::Char('t'), Action::CancelKeySequence)]
        #[case("inspector", Key::Char('b'), Action::CancelKeySequence)]
        #[case("focus_mode", Key::Char('z'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
        #[case("focus_mode", Key::Char('t'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
        #[case("focus_mode", Key::Char('b'), Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
        fn scroll_to_cursor_zztb(
            #[case] ctx_name: &str,
            #[case] key: Key,
            #[case] expected: Action,
        ) {
            let mut state = match ctx_name {
                "explorer" => explorer_ctx(),
                "result_scroll" => result_scroll_ctx(),
                "result_cell_active" => result_cell_active_ctx(),
                "inspector" => inspector_ctx(),
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
        #[case("result_cell_active", result_cell_active_ctx())]
        #[case("inspector", inspector_ctx())]
        #[case("focus_mode", focus_mode_ctx())]
        fn z_prefix_returns_begin_key_sequence(#[case] ctx_name: &str, #[case] state: AppState) {
            let actual = handle_normal_mode(combo(Key::Char('z')), &state);
            assert_action(actual, Action::BeginKeySequence(Prefix::Z), ctx_name, "z");
        }
    }
}
