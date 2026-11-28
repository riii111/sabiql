use crate::update::er::diagram::er_open_diagram;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::TableSummary;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;

pub(in crate::update) fn open_modal_er_table_picker(
    state: &mut AppState,
    now: Instant,
) -> Vec<Effect> {
    if state.session.metadata().is_none() {
        state.ui.request_er_picker_after_metadata();
        state
            .messages
            .set_success_at("Waiting for metadata...".to_string(), now);
        return vec![];
    }
    state.ui.reset_er_picker_request();
    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.er_picker_mut().clear_filter_and_reset();
    vec![]
}

pub(in crate::update) fn close_modal_er_table_picker(state: &mut AppState) -> Vec<Effect> {
    state.modal.set_mode(InputMode::Normal);
    state.ui.er_picker_mut().clear_filter();
    state.ui.reset_er_picker_request();
    vec![]
}

pub(in crate::update) fn er_toggle_selection(state: &mut AppState) -> Vec<Effect> {
    let filtered = state.er_filtered_tables();
    if let Some(table) = filtered.get(state.ui.er_picker().selected()) {
        let name = table.qualified_name();
        state.ui.toggle_er_selected_table(name);
    }
    vec![]
}

pub(in crate::update) fn er_select_all(state: &mut AppState) -> Vec<Effect> {
    let all_tables: Vec<String> = state
        .tables()
        .iter()
        .map(TableSummary::qualified_name)
        .collect();
    if state.ui.er_selected_tables().len() == all_tables.len() {
        state.ui.clear_er_selected_tables();
    } else {
        state.ui.replace_er_selected_tables(all_tables);
    }
    vec![]
}

pub(in crate::update) fn er_confirm_selection(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if state.ui.er_selected_tables().is_empty() {
        state.messages.set_error("No tables selected".to_string());
        return vec![];
    }
    state
        .er_preparation
        .set_targets(state.ui.er_selected_tables().iter().cloned().collect());
    state.modal.set_mode(InputMode::Normal);
    state.ui.er_picker_mut().clear_filter();
    state.ui.clear_er_selected_tables();
    er_open_diagram(state, now)
}
