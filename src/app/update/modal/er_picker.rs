use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::{TextInputEditing, TextInputState};
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::require_er_diagram_enabled;

pub(super) fn reduce_er_picker(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::ErTablePicker) => {
            if let Some(result) = require_er_diagram_enabled(state, now) {
                return result;
            }
            if state.session.metadata().is_none() {
                state.ui.request_er_picker_after_metadata();
                state
                    .messages
                    .set_success_at("Waiting for metadata...".to_string(), now);
                return DispatchResult::handled();
            }
            state.ui.reset_er_picker_request();
            state.modal.set_mode(InputMode::ErTablePicker);
            state.ui.er_picker_mut().clear_filter_and_reset();
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::ErTablePicker) => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker_mut().clear_filter();
            state.ui.reset_er_picker_request();
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        } => {
            state.ui.er_picker_mut().insert_filter_char(*c);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker_mut().backspace_filter();
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker_mut().edit_filter(TextInputState::delete);
            DispatchResult::handled()
        }
        Action::TextKill {
            target: InputTarget::ErFilter,
            direction,
        } => {
            let killed = state
                .ui
                .er_picker_mut()
                .edit_filter(|input| input.kill(*direction));
            state.record_kill(killed);
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::ErFilter,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state
                    .ui
                    .er_picker_mut()
                    .edit_filter(|input| input.yank(&killed));
            }
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::ErFilter,
            direction: movement,
        } => {
            state.ui.er_picker_mut().move_filter_cursor(*movement);
            DispatchResult::handled()
        }
        Action::ErToggleSelection => {
            let filtered = state.er_filtered_tables();
            if let Some(table) = filtered.get(state.ui.er_picker().selected()) {
                let name = table.qualified_name();
                state.ui.toggle_er_selected_table(name);
            }
            DispatchResult::handled()
        }
        Action::ErSelectAll => {
            let all_tables: Vec<String> =
                state.tables().iter().map(|t| t.qualified_name()).collect();
            if state.ui.er_selected_tables().len() == all_tables.len() {
                state.ui.clear_er_selected_tables();
            } else {
                state.ui.replace_er_selected_tables(all_tables);
            }
            DispatchResult::handled()
        }
        Action::ErConfirmSelection => {
            if state.ui.er_selected_tables().is_empty() {
                state
                    .messages
                    .set_error_at("No tables selected".to_string(), now);
                return DispatchResult::handled();
            }
            state
                .er_preparation
                .set_targets(state.ui.er_selected_tables().iter().cloned().collect());
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker_mut().clear_filter();
            state.ui.clear_er_selected_tables();
            DispatchResult::handled_with(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
        }
        _ => DispatchResult::pass(),
    }
}
