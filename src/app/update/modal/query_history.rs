use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ListMotion, ListTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(super) fn reduce_query_history_picker(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::QueryHistoryPicker) => {
            if state.modal.active_mode() == InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            if state.session.active_connection_id().is_none() {
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

            let scope = state
                .session
                .query_history_scope()
                .expect("active connection checked above");
            DispatchResult::handled_with(vec![Effect::LoadQueryHistory {
                project_name: state.project_name().to_string(),
                scope,
            }])
        }
        Action::CloseModal(ModalKind::QueryHistoryPicker) => {
            state.modal.pop_mode();
            state.query_history_picker.reset();
            DispatchResult::handled()
        }
        Action::QueryHistoryLoaded(scope, entries) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            if state.session.query_history_scope().as_ref() != Some(scope) {
                return DispatchResult::handled();
            }
            state.query_history_picker.replace_entries(entries);
            DispatchResult::handled()
        }
        Action::QueryHistoryLoadFailed(scope, error) => {
            if state.modal.active_mode() != InputMode::QueryHistoryPicker {
                return DispatchResult::handled();
            }
            if state.session.query_history_scope().as_ref() != Some(scope) {
                return DispatchResult::handled();
            }
            state.messages.set_error(error.clone());
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
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
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
