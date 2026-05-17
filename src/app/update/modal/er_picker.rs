use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_er_picker(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::ErTablePicker) => {
            if state.session.metadata().is_none() {
                state.ui.request_er_picker_after_metadata();
                state.set_success("Waiting for metadata...".to_string());
                return DispatchResult::handled();
            }
            state.ui.reset_er_picker_request();
            state.modal.set_mode(InputMode::ErTablePicker);
            state.ui.er_picker.clear_filter_and_reset();
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::ErTablePicker) => {
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.clear_filter();
            state.ui.reset_er_picker_request();
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::ErFilter,
            ch: c,
        } => {
            state.ui.er_picker.insert_filter_char(*c);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::ErFilter,
        } => {
            state.ui.er_picker.backspace_filter();
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::ErFilter,
            direction: movement,
        } => {
            state.ui.er_picker.move_filter_cursor(*movement);
            DispatchResult::handled()
        }
        Action::ErToggleSelection => {
            let filtered = state.er_filtered_tables();
            if let Some(table) = filtered.get(state.ui.er_picker.selected()) {
                let name = table.qualified_name();
                if !state.ui.er_selected_tables.remove(&name) {
                    state.ui.er_selected_tables.insert(name);
                }
            }
            DispatchResult::handled()
        }
        Action::ErSelectAll => {
            let all_tables: Vec<String> =
                state.tables().iter().map(|t| t.qualified_name()).collect();
            if state.ui.er_selected_tables.len() == all_tables.len() {
                state.ui.er_selected_tables.clear();
            } else {
                state.ui.er_selected_tables = all_tables.into_iter().collect();
            }
            DispatchResult::handled()
        }
        Action::ErConfirmSelection => {
            if state.ui.er_selected_tables.is_empty() {
                state.set_error("No tables selected".to_string());
                return DispatchResult::handled();
            }
            state.er_preparation.target_tables =
                state.ui.er_selected_tables.iter().cloned().collect();
            state.modal.set_mode(InputMode::Normal);
            state.ui.er_picker.clear_filter();
            state.ui.er_selected_tables.clear();
            DispatchResult::handled_with(vec![Effect::DispatchActions(vec![Action::ErOpenDiagram])])
        }
        _ => DispatchResult::pass(),
    }
}
