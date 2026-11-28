use crate::cmd::effect::Effect;
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::ListMotion;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(in crate::update) fn open_modal_query_history_picker(state: &mut AppState) -> Vec<Effect> {
    if !state.can_open_query_history() {
        return vec![];
    }

    state.query_history_picker.reset();
    state.modal.push_mode(InputMode::QueryHistoryPicker);

    let scope = state
        .session
        .query_history_scope()
        .expect("active connection checked above");
    vec![Effect::LoadQueryHistory {
        project_name: state.project_name().to_string(),
        scope,
    }]
}

pub(in crate::update) fn close_modal_query_history_picker(state: &mut AppState) -> Vec<Effect> {
    state.modal.pop_mode();
    state.query_history_picker.reset();
    vec![]
}

pub(in crate::update) fn query_history_loaded(
    state: &mut AppState,
    scope: QueryHistoryScope,
    outcome: Result<Vec<QueryHistoryEntry>, String>,
) -> Vec<Effect> {
    if state.modal.active_mode() != InputMode::QueryHistoryPicker {
        return vec![];
    }
    if state.session.query_history_scope().as_ref() != Some(&scope) {
        return vec![];
    }
    match outcome {
        Ok(entries) => state.query_history_picker.replace_entries(&entries),
        Err(error) => state.messages.set_error(error),
    }
    vec![]
}

pub(in crate::update) fn list_select_query_history(
    state: &mut AppState,
    motion: ListMotion,
) -> Vec<Effect> {
    match motion {
        ListMotion::Next => {
            state.query_history_picker.select_next();
        }
        ListMotion::Previous => {
            state.query_history_picker.select_previous();
        }
    }
    vec![]
}

pub(in crate::update) fn query_history_confirm_selection(state: &mut AppState) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let grouped = state.query_history_picker.grouped_filtered_entries();
    let selected = state.query_history_picker.clamped_selected();
    let query = grouped.get(selected).map(|g| g.entry.query.clone());
    let origin = state.modal.pop_mode();

    state.query_history_picker.reset();

    let Some(query) = query else {
        return vec![];
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
    vec![]
}
