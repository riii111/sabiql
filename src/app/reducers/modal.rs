//! Modal sub-reducer: modal/overlay toggles and confirm dialog.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducer::reduce;
use crate::app::state::AppState;

/// Handles modal/overlay toggles and confirm dialog actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_modal(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenTablePicker => {
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.filter_input.clear();
            state.ui.picker_selected = 0;
            Some(vec![])
        }
        Action::CloseTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::OpenCommandPalette => {
            state.ui.input_mode = InputMode::CommandPalette;
            state.ui.picker_selected = 0;
            Some(vec![])
        }
        Action::CloseCommandPalette => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::OpenHelp => {
            state.ui.input_mode = if state.ui.input_mode == InputMode::Help {
                InputMode::Normal
            } else {
                InputMode::Help
            };
            Some(vec![])
        }
        Action::CloseHelp => {
            state.ui.input_mode = InputMode::Normal;
            state.ui.help_scroll_offset = 0;
            Some(vec![])
        }
        Action::HelpScrollUp => {
            state.ui.help_scroll_offset = state.ui.help_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::HelpScrollDown => {
            let max_scroll = state.ui.help_max_scroll();
            if state.ui.help_scroll_offset < max_scroll {
                state.ui.help_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::CloseSqlModal => {
            state.ui.input_mode = InputMode::Normal;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            Some(vec![])
        }
        Action::Escape => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }

        // Confirm Dialog
        Action::OpenConfirmDialog => {
            state.ui.input_mode = InputMode::ConfirmDialog;
            Some(vec![])
        }
        Action::CloseConfirmDialog => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ConfirmDialogConfirm => {
            let action = std::mem::replace(&mut state.confirm_dialog.on_confirm, Action::None);
            state.confirm_dialog.on_cancel = Action::None;
            let return_mode =
                std::mem::replace(&mut state.confirm_dialog.return_mode, InputMode::Normal);
            state.ui.input_mode = return_mode;
            Some(reduce(state, action, now))
        }
        Action::ConfirmDialogCancel => {
            let action = std::mem::replace(&mut state.confirm_dialog.on_cancel, Action::None);
            state.confirm_dialog.on_confirm = Action::None;
            let return_mode =
                std::mem::replace(&mut state.confirm_dialog.return_mode, InputMode::Normal);
            state.ui.input_mode = return_mode;
            Some(reduce(state, action, now))
        }

        _ => None,
    }
}
