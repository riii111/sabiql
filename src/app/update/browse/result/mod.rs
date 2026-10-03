mod cell_detail;
mod edit;
mod json;
mod row_detail;
mod scroll;
mod selection;
mod yank;

use std::time::Instant;

use crate::domain::QueryValue;
use crate::model::app_state::AppState;
use crate::policy::preview_cell_text::CellPresentationPolicy;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn dispatch_result(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> DispatchResult {
    scroll::reduce_scroll(state, action)
        .or_else(|| selection::reduce_selection(state, action, now))
        .or_else(|| edit::reduce_edit(state, action, now))
        .or_else(|| yank::reduce_yank(state, action, services, now))
        .or_else(|| cell_detail::reduce_cell_detail(state, action, now))
        .or_else(|| json::reduce_json(state, action, now))
        .or_else(|| row_detail::reduce_row_detail(state, action, now))
}

fn selected_cell_uses_json_detail_modal(state: &AppState) -> bool {
    let Some(col_idx) = state.result_interaction.selection().cell() else {
        return false;
    };
    let Some(row_idx) = state.result_interaction.selection().row() else {
        return false;
    };
    let Some(result) = state.query.visible_result() else {
        return false;
    };
    if matches!(result.value_at(row_idx, col_idx), Some(QueryValue::Null)) {
        return false;
    }
    let Some(column_data_type) = selected_column_data_type(state, col_idx) else {
        return false;
    };
    let policy = CellPresentationPolicy::new(
        state.session.active_database_type_or_default(),
        column_data_type,
        "",
    );
    policy.uses_json_detail_modal()
}

fn selected_column_data_type(state: &AppState, col_idx: usize) -> Option<&str> {
    let table_detail = state.session.table_detail()?;
    if !state.query.pagination.matches_table(table_detail) {
        return None;
    }
    state
        .visible_preview_column(col_idx)
        .map(|column| column.data_type.as_str())
}
