use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::db_capabilities::DbCapabilities;
use crate::model::sql_editor::modal::SqlModalTab;
use crate::policy::write::sql_risk::split_statements;
use crate::services::AppServices;

pub(super) fn active_capabilities<'a>(
    state: &'a AppState,
    services: &'a AppServices,
) -> &'a DbCapabilities {
    if state.session.active_database_type().is_some() {
        state.session.active_db_capabilities()
    } else {
        &services.db_capabilities
    }
}

pub(super) fn is_multi_statement(content: &str) -> bool {
    split_statements(content).len() > 1
}

pub(super) fn mark_explain_unavailable(state: &mut AppState, services: &AppServices) {
    state
        .explain
        .set_error("EXPLAIN is unavailable for this database".to_string());
    let tab =
        active_capabilities(state, services).normalize_sql_modal_tab(state.sql_modal.active_tab());
    state.sql_modal.set_active_tab(tab);
}

pub(super) fn show_explain_error_on_plan(state: &mut AppState, message: impl Into<String>) {
    state.explain.set_error(message.into());
    state.sql_modal.set_active_tab(SqlModalTab::Plan);
}

pub(super) fn begin_explain_running(state: &mut AppState, now: Instant) -> u64 {
    state.sql_modal.begin_adhoc_running();
    state.sql_modal.set_active_tab(SqlModalTab::Plan);
    state.explain.reset_for_new_run();
    state.query.begin_running(now)
}

pub(super) fn finish_explain_success(
    state: &mut AppState,
    plan_text: String,
    is_analyze: bool,
    execution_time_ms: u64,
    query: &str,
) {
    state
        .explain
        .set_plan(plan_text, is_analyze, execution_time_ms, query);
    state.sql_modal.enter_normal();
    state.sql_modal.set_active_tab(SqlModalTab::Plan);
    state.query.mark_idle();
}

pub(super) fn finish_explain_error(state: &mut AppState, error: impl Into<String>) {
    state.explain.set_error(error.into());
    state.sql_modal.enter_normal();
    state.sql_modal.set_active_tab(SqlModalTab::Plan);
    state.query.mark_idle();
}

pub(super) fn reject_unsupported_explain(state: &mut AppState, services: &AppServices) -> bool {
    if active_capabilities(state, services).supports_explain() {
        return false;
    }

    mark_explain_unavailable(state, services);
    true
}
