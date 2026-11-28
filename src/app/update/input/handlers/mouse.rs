use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::ports::inbound::{Key, KeyCombo, MouseInput, MouseKind};
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget, SelectMotion};
use crate::update::input::keymap::{KeyContext, key_action};

pub(super) fn handle_mouse_event(input: MouseInput, state: &AppState) -> Action {
    if state.input_mode() == InputMode::Normal {
        browse_mouse_action(input, state)
    } else if scrolls_like_arrow_keys(state.input_mode()) {
        modal_wheel_action(input.kind, state)
    } else {
        Action::None
    }
}

// The pane under the pointer reacts, as in GUI apps, even when another pane has keyboard focus.
fn browse_mouse_action(input: MouseInput, state: &AppState) -> Action {
    let Some(pane) = state.ui.pane_areas().pane_at(input.column, input.row) else {
        return Action::None;
    };
    match input.kind {
        MouseKind::LeftClick => Action::SetFocusedPane(pane),
        MouseKind::ScrollUp => vertical_step(pane, ScrollDirection::Up),
        MouseKind::ScrollDown => vertical_step(pane, ScrollDirection::Down),
        MouseKind::ScrollLeft => horizontal_step(pane, ScrollDirection::Left),
        MouseKind::ScrollRight => horizontal_step(pane, ScrollDirection::Right),
    }
}

// The explorer has no free-scrolling viewport; its list follows the selection like `j`/`k`.
fn vertical_step(pane: FocusedPane, direction: ScrollDirection) -> Action {
    match (pane, direction) {
        (FocusedPane::Explorer, ScrollDirection::Down) => Action::Select(SelectMotion::Next),
        (FocusedPane::Explorer, _) => Action::Select(SelectMotion::Previous),
        (FocusedPane::Inspector, _) => line_scroll(ScrollTarget::Inspector, direction),
        (FocusedPane::Result, _) => line_scroll(ScrollTarget::Result, direction),
    }
}

fn horizontal_step(pane: FocusedPane, direction: ScrollDirection) -> Action {
    let target = match pane {
        FocusedPane::Explorer => ScrollTarget::Explorer,
        FocusedPane::Inspector => ScrollTarget::Inspector,
        FocusedPane::Result => ScrollTarget::Result,
    };
    line_scroll(target, direction)
}

fn line_scroll(target: ScrollTarget, direction: ScrollDirection) -> Action {
    Action::Scroll {
        target,
        direction,
        amount: ScrollAmount::Line,
    }
}

// Modals reuse their arrow-key bindings so the wheel scrolls or moves the list exactly like the keyboard.
fn modal_wheel_action(kind: MouseKind, state: &AppState) -> Action {
    let key = match kind {
        MouseKind::ScrollUp => Key::Up,
        MouseKind::ScrollDown => Key::Down,
        MouseKind::ScrollLeft | MouseKind::ScrollRight | MouseKind::LeftClick => {
            return Action::None;
        }
    };
    key_action(KeyCombo::plain(key), &KeyContext::from_state(state))
}

// Text editors and forms also bind the arrow keys, but there they move a cursor or field, not a view.
#[deny(
    clippy::wildcard_enum_match_arm,
    reason = "A new mode must decide whether the wheel acts like its arrow keys."
)]
fn scrolls_like_arrow_keys(mode: InputMode) -> bool {
    match mode {
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
        InputMode::Normal
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::Settings
        | InputMode::SqlModal
        | InputMode::ConnectionSetup
        | InputMode::JsonEdit => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::render_output::{PaneArea, PaneAreas};
    use crate::update::action::ListMotion;
    use crate::update::action::ListTarget;
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
    fn wheel_over_result_scrolls_result_without_focusing_it() {
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
        assert_eq!(state.ui.focused_pane(), FocusedPane::Explorer);
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

    #[rstest]
    #[case(MouseKind::ScrollDown, SelectMotion::Next)]
    #[case(MouseKind::ScrollUp, SelectMotion::Previous)]
    fn wheel_over_explorer_moves_selection(#[case] kind: MouseKind, #[case] motion: SelectMotion) {
        let state = browse_state();

        let action = handle_mouse_event(mouse(kind, EXPLORER_CELL), &state);

        assert!(matches!(action, Action::Select(m) if m == motion));
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
