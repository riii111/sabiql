use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::SqlModalTab;
use crate::ports::inbound::{Key, KeyCombo, MouseInput, MouseKind};
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::input::keymap::{KeyContext, key_action};

pub(super) fn handle_mouse_event(input: MouseInput, state: &AppState) -> Action {
    if state.input_mode() == InputMode::Normal {
        browse_mouse_action(input, state)
    } else if scrolls_like_arrow_keys(state) {
        modal_wheel_action(input.kind, state)
    } else {
        Action::None
    }
}

fn browse_mouse_action(input: MouseInput, state: &AppState) -> Action {
    // The pane under the pointer reacts even when another pane has keyboard focus, as in GUI apps.
    let Some(pane) = state.ui.pane_areas().pane_at(input.column, input.row) else {
        return Action::None;
    };
    let target = match pane {
        FocusedPane::Explorer => ScrollTarget::Explorer,
        FocusedPane::Inspector => ScrollTarget::Inspector,
        FocusedPane::Result => ScrollTarget::Result,
    };
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
    let key = match kind {
        MouseKind::ScrollUp => Key::Up,
        MouseKind::ScrollDown => Key::Down,
        MouseKind::ScrollLeft | MouseKind::ScrollRight | MouseKind::LeftClick => {
            return Action::None;
        }
    };
    // Reusing the arrow-key binding keeps the wheel identical to the keyboard in every modal.
    key_action(KeyCombo::plain(key), &KeyContext::from_state(state))
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
        // The plan and comparison tabs are read-only views, while the SQL tab is an editor.
        // The key context reports the tab after engine support is applied, which is the tab keys act on.
        InputMode::SqlModal => matches!(
            KeyContext::from_state(state).sql_tab(),
            Some(SqlModalTab::Plan | SqlModalTab::Compare)
        ),
        // Text editors and forms bind the arrow keys to a cursor or field, not to a view.
        InputMode::Normal
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
        state.ui.set_pane_areas(PaneAreas {
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
        });
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
            .map(|i| TableSummary::new("public".to_string(), format!("table_{i}"), Some(0), false))
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
        state.ui.set_focused_pane(FocusedPane::Result);

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
}
