use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::ports::inbound::{Key, KeyCombo, MouseInput, MouseKind};
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::input::keymap::{KeyContext, key_action};

pub(super) fn handle_mouse_event(input: MouseInput, state: &AppState) -> Action {
    if state.input_mode() == InputMode::Normal {
        browse_mouse_action(input, state)
    } else {
        modal_wheel_action(input.kind, state)
    }
}

fn browse_mouse_action(input: MouseInput, state: &AppState) -> Action {
    // The pane under the pointer reacts even when another pane has keyboard focus, as in GUI apps.
    let Some(pane) = state.ui.pane_areas.pane_at(input.column, input.row) else {
        return Action::None;
    };
    let target = match pane {
        FocusedPane::Explorer => ScrollTarget::Explorer,
        FocusedPane::Inspector => ScrollTarget::Inspector,
        FocusedPane::Result => ScrollTarget::Result,
    };
    // With a cell selected, horizontal steps move the selection as the arrow keys do, so the cell that copy and edit act on stays in view.
    if pane == FocusedPane::Result && state.result_interaction.selection().cell().is_some() {
        match input.kind {
            MouseKind::ScrollLeft => return Action::ResultCellLeft,
            MouseKind::ScrollRight => return Action::ResultCellRight,
            MouseKind::LeftClick | MouseKind::ScrollUp | MouseKind::ScrollDown => {}
        }
    }
    match input.kind {
        MouseKind::LeftClick => Action::SetFocusedPane(pane),
        MouseKind::ScrollUp => line_scroll(target, ScrollDirection::Up),
        MouseKind::ScrollDown => line_scroll(target, ScrollDirection::Down),
        MouseKind::ScrollLeft => line_scroll(target, ScrollDirection::Left),
        MouseKind::ScrollRight => line_scroll(target, ScrollDirection::Right),
    }
}

fn line_scroll(target: ScrollTarget, direction: ScrollDirection) -> Action {
    Action::Scroll {
        target,
        direction,
        amount: ScrollAmount::Line,
    }
}

fn modal_wheel_action(kind: MouseKind, state: &AppState) -> Action {
    let (key, vertical) = match kind {
        MouseKind::ScrollUp => (Key::Up, true),
        MouseKind::ScrollDown => (Key::Down, true),
        MouseKind::ScrollLeft => (Key::Left, false),
        MouseKind::ScrollRight => (Key::Right, false),
        MouseKind::LeftClick => return Action::None,
    };
    // Reusing the arrow-key binding keeps the wheel identical to the keyboard in every modal.
    let action = key_action(KeyCombo::plain(key), &KeyContext::from_state(state));
    // The wheel moves views and selections only, so arrow keys bound to a text cursor or field switch stay keyboard-only.
    let moves_a_view = match action {
        Action::Scroll { .. } => true,
        Action::SettingsSelectNext | Action::SettingsSelectPrevious => vertical,
        _ => vertical && scrolls_like_arrow_keys(state),
    };
    if moves_a_view { action } else { Action::None }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    reason = "A new mode must decide whether the wheel acts like its arrow keys."
)]
fn scrolls_like_arrow_keys(state: &AppState) -> bool {
    match state.input_mode() {
        InputMode::TablePicker
        | InputMode::CommandPalette
        | InputMode::Help
        | InputMode::ConnectionError
        | InputMode::ConfirmDialog
        | InputMode::ConnectionSelector
        | InputMode::ErTablePicker
        | InputMode::QueryHistoryPicker
        | InputMode::JsonDetail
        | InputMode::CellDetail
        | InputMode::SqliteDiagnostics
        | InputMode::RowDetail => true,
        // These also bind the arrow keys to a cursor or field, so only their scroll and selection actions pass.
        InputMode::Normal
        | InputMode::SqlModal
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::Settings
        | InputMode::ConnectionSetup
        | InputMode::JsonEdit => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::model::shared::render_output::{PaneArea, PaneAreas};
    use crate::model::sql_editor::modal::SqlModalTab;
    use crate::services::AppServices;
    use crate::update::action::ListMotion;
    use crate::update::action::ListTarget;
    use crate::update::reducer::reduce;
    use crate::update::test_fixtures;
    use rstest::rstest;

    const EXPLORER_CELL: (u16, u16) = (5, 5);
    const INSPECTOR_CELL: (u16, u16) = (30, 5);
    const RESULT_CELL: (u16, u16) = (30, 20);

    fn browse_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.ui.pane_areas = PaneAreas {
            explorer: Some(PaneArea {
                x: 0,
                y: 1,
                width: 20,
                height: 28,
            }),
            inspector: Some(PaneArea {
                x: 20,
                y: 1,
                width: 60,
                height: 14,
            }),
            result: Some(PaneArea {
                x: 20,
                y: 15,
                width: 60,
                height: 14,
            }),
        };
        state
    }

    fn mouse(kind: MouseKind, (column, row): (u16, u16)) -> MouseInput {
        MouseInput { kind, column, row }
    }

    #[rstest]
    #[case(EXPLORER_CELL, FocusedPane::Explorer)]
    #[case(INSPECTOR_CELL, FocusedPane::Inspector)]
    #[case(RESULT_CELL, FocusedPane::Result)]
    fn click_focuses_pane_under_pointer(#[case] cell: (u16, u16), #[case] pane: FocusedPane) {
        let state = browse_state();

        let action = handle_mouse_event(mouse(MouseKind::LeftClick, cell), &state);

        assert!(matches!(action, Action::SetFocusedPane(p) if p == pane));
    }

    #[test]
    fn click_outside_panes_does_nothing() {
        let state = browse_state();

        let action = handle_mouse_event(mouse(MouseKind::LeftClick, (5, 0)), &state);

        assert!(matches!(action, Action::None));
    }

    #[test]
    fn wheel_over_result_scrolls_result() {
        let state = browse_state();

        let action = handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state);

        assert!(matches!(
            action,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn wheel_over_inspector_scrolls_inspector() {
        let state = browse_state();

        let action = handle_mouse_event(mouse(MouseKind::ScrollUp, INSPECTOR_CELL), &state);

        assert!(matches!(
            action,
            Action::Scroll {
                target: ScrollTarget::Inspector,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line,
            }
        ));
    }

    fn with_tables(state: &mut AppState, count: usize) {
        let tables = (0..count)
            .map(|i| TableSummary::new("public".to_string(), format!("table_{i}"), Some(0)))
            .collect();
        state.session.set_metadata(Some(Arc::new({
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.table_summaries = tables;
            metadata
        })));
        state.ui.set_explorer_pane_height(10);
        state.ui.set_explorer_selection(Some(1));
    }

    #[rstest]
    #[case(MouseKind::ScrollDown, 2)]
    #[case(MouseKind::ScrollUp, 0)]
    fn wheel_over_unfocused_explorer_moves_selection(
        #[case] kind: MouseKind,
        #[case] expected: usize,
    ) {
        let mut state = browse_state();
        with_tables(&mut state, 3);
        state.focus_pane(FocusedPane::Result);

        let action = handle_mouse_event(mouse(kind, EXPLORER_CELL), &state);
        reduce(&mut state, action, Instant::now(), &AppServices::stub());

        assert_eq!(state.ui.explorer_selected(), expected);
        assert_eq!(state.ui.focused_pane(), FocusedPane::Result);
    }

    #[test]
    fn horizontal_wheel_scrolls_result_columns() {
        let state = browse_state();

        let action = handle_mouse_event(mouse(MouseKind::ScrollRight, RESULT_CELL), &state);

        assert!(matches!(
            action,
            Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Right,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn wheel_in_picker_moves_list_like_arrow_keys() {
        let mut state = browse_state();
        state.modal.set_mode(InputMode::TablePicker);

        let action = handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state);

        assert!(matches!(
            action,
            Action::ListSelect {
                target: ListTarget::TablePicker,
                motion: ListMotion::Next,
            }
        ));
    }

    #[test]
    fn wheel_in_sql_plan_tab_scrolls_plan() {
        let mut state = browse_state();
        test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.set_active_tab(SqlModalTab::Plan);

        let action = handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state);

        assert!(matches!(
            action,
            Action::Scroll {
                target: ScrollTarget::ExplainPlan,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn wheel_in_plan_tab_unsupported_by_engine_is_ignored() {
        let mut state = browse_state();
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.set_active_tab(SqlModalTab::Plan);

        let action = handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state);

        assert!(matches!(action, Action::None));
    }

    #[rstest]
    #[case(InputMode::SqlModal)]
    #[case(InputMode::ConnectionSetup)]
    #[case(InputMode::CellEdit)]
    fn wheel_in_text_editors_is_ignored(#[case] mode: InputMode) {
        let mut state = browse_state();
        state.modal.set_mode(mode);

        let action = handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state);

        assert!(matches!(action, Action::None));
    }

    #[test]
    fn click_in_modal_is_ignored() {
        let mut state = browse_state();
        state.modal.set_mode(InputMode::Help);

        let action = handle_mouse_event(mouse(MouseKind::LeftClick, RESULT_CELL), &state);

        assert!(matches!(action, Action::None));
    }

    #[test]
    fn horizontal_wheel_with_selected_cell_moves_the_cell() {
        let mut state = browse_state();
        state.result_interaction.activate_cell(0, 0);

        assert!(matches!(
            handle_mouse_event(mouse(MouseKind::ScrollRight, RESULT_CELL), &state),
            Action::ResultCellRight
        ));
        assert!(matches!(
            handle_mouse_event(mouse(MouseKind::ScrollLeft, RESULT_CELL), &state),
            Action::ResultCellLeft
        ));
    }

    #[test]
    fn wheel_in_settings_moves_the_selection() {
        let mut state = browse_state();
        state.modal.set_mode(InputMode::Settings);

        assert!(matches!(
            handle_mouse_event(mouse(MouseKind::ScrollDown, RESULT_CELL), &state),
            Action::SettingsSelectNext
        ));
    }

    #[test]
    fn horizontal_wheel_in_help_scrolls_sideways() {
        let mut state = browse_state();
        state.modal.set_mode(InputMode::Help);

        assert!(matches!(
            handle_mouse_event(mouse(MouseKind::ScrollRight, RESULT_CELL), &state),
            Action::Scroll {
                target: ScrollTarget::Help,
                direction: ScrollDirection::Right,
                ..
            }
        ));
    }
}
