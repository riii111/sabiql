use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, InputTarget, ListMotion, ListTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_query_history_picker(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::QueryHistoryPicker) => {
            if state.modal.active_mode() == InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            if state.session.active_connection_id.is_none() {
                return DispatchResult::handled();
            }
            if state.query.is_running() {
                return DispatchResult::handled();
            }
            if state.modal.active_mode() == InputMode::ConfirmDialog {
                return DispatchResult::handled();
            }
            if state.sql_modal.completion().visible
                && !state.sql_modal.completion().candidates.is_empty()
            {
                return DispatchResult::handled();
            }

            state.query_history_picker.reset();
            state.modal.push_mode(InputMode::QueryHistoryPicker);

            let conn_id = state.session.active_connection_id.as_ref().unwrap();
            DispatchResult::handled_with(vec![Effect::LoadQueryHistory {
                project_name: state.runtime.project_name.clone(),
                connection_id: conn_id.clone(),
            }])
        }
        Action::CloseModal(ModalKind::QueryHistoryPicker) => {
            state.modal.pop_mode();
            state.query_history_picker.reset();
            DispatchResult::handled()
        }
        Action::QueryHistoryLoaded(conn_id, entries) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            if state.session.active_connection_id.as_ref() != Some(conn_id) {
                return DispatchResult::handled();
            }
            state.query_history_picker.replace_entries(entries);
            DispatchResult::handled()
        }
        Action::QueryHistoryLoadFailed(e) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            state.messages.set_error_at(e.to_string(), now);
            DispatchResult::handled()
        }
        Action::QueryHistoryAppendFailed(_) => DispatchResult::handled(),
        Action::TextInput {
            target: InputTarget::QueryHistoryFilter,
            ch: c,
        } => {
            state.query_history_picker.insert_filter_char(*c);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::QueryHistoryFilter,
        } => {
            state.query_history_picker.backspace_filter();
            DispatchResult::handled()
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Next,
        } => {
            state.query_history_picker.select_next();
            DispatchResult::handled()
        }
        Action::ListSelect {
            target: ListTarget::QueryHistory,
            motion: ListMotion::Previous,
        } => {
            state.query_history_picker.select_previous();
            DispatchResult::handled()
        }
        Action::QueryHistoryConfirmSelection => {
            let grouped = state.query_history_picker.grouped_filtered_entries();
            let selected = state.query_history_picker.clamped_selected();
            let query = grouped.get(selected).map(|g| g.entry.query.clone());
            let origin = state.modal.pop_mode();

            state.query_history_picker.reset();

            let Some(query) = query else {
                return DispatchResult::handled();
            };

            match origin {
                InputMode::Normal => {
                    state.modal.set_mode(InputMode::SqlModal);
                    state.sql_modal.load_query_from_history(query);
                }
                InputMode::SqlModal => {
                    state.sql_modal.load_query_from_history(query);
                }
                _ => {}
            }
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
