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
            state.ui.reset_picker_selection();
            Some(vec![])
        }
        Action::CloseTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::OpenCommandPalette => {
            state.ui.input_mode = InputMode::CommandPalette;
            state.ui.reset_picker_selection();
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
        Action::OpenErTablePicker => {
            if state.cache.metadata.is_none() {
                state.ui.pending_er_picker = true;
                state.set_success("Waiting for metadata...".to_string());
                return Some(vec![]);
            }
            state.ui.pending_er_picker = false;
            state.ui.er_selected_tables.clear();
            state.ui.input_mode = InputMode::ErTablePicker;
            state.ui.er_filter_input.clear();
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::CloseErTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            state.ui.er_filter_input.clear();
            state.ui.er_selected_tables.clear();
            state.ui.pending_er_picker = false;
            Some(vec![])
        }
        Action::ErFilterInput(c) => {
            state.ui.er_filter_input.push(*c);
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::ErFilterBackspace => {
            state.ui.er_filter_input.pop();
            state.ui.reset_er_picker_selection();
            Some(vec![])
        }
        Action::ErToggleSelection => {
            let filtered = state.er_filtered_tables();
            if let Some(table) = filtered.get(state.ui.er_picker_selected) {
                let name = table.qualified_name();
                if !state.ui.er_selected_tables.remove(&name) {
                    state.ui.er_selected_tables.insert(name);
                }
            }
            Some(vec![])
        }
        Action::ErSelectAll => {
            let all_tables: Vec<String> =
                state.tables().iter().map(|t| t.qualified_name()).collect();
            if state.ui.er_selected_tables.len() == all_tables.len() {
                state.ui.er_selected_tables.clear();
            } else {
                state.ui.er_selected_tables = all_tables.into_iter().collect();
            }
            Some(vec![])
        }
        Action::ErConfirmSelection => {
            if state.ui.er_selected_tables.is_empty() {
                state.set_error("No tables selected".to_string());
                return Some(vec![]);
            }
            state.er_preparation.target_tables =
                state.ui.er_selected_tables.iter().cloned().collect();
            state.ui.input_mode = InputMode::Normal;
            state.ui.er_filter_input.clear();
            state.ui.er_selected_tables.clear();
            Some(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
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
            state.pending_write_preview = None;
            state.query.pending_delete_refresh_target = None;
            let return_mode =
                std::mem::replace(&mut state.confirm_dialog.return_mode, InputMode::Normal);
            state.ui.input_mode = return_mode;
            Some(reduce(state, action, now))
        }

        _ => None,
    }
}
