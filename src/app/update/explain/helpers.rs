use std::time::Instant;

use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::policy::sql::statement_classifier;
use crate::policy::write::sql_risk::split_statements_for_database;

pub(super) fn explain_unsupported_query_message(database_type: DatabaseType) -> &'static str {
    match database_type {
        DatabaseType::SQLite => "EXPLAIN QUERY PLAN supports SELECT statements only",
        DatabaseType::PostgreSQL => "EXPLAIN is unavailable for this statement",
    }
}

pub(super) fn explain_unsupported_sqlite_query_message(content: &str) -> &'static str {
    if statement_classifier::first_keyword(content.trim())
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("EXPLAIN"))
    {
        "EXPLAIN QUERY PLAN is added automatically; enter a SELECT query only"
    } else {
        explain_unsupported_query_message(DatabaseType::SQLite)
    }
}

pub(super) fn explain_unsupported_analyze_message(database_type: DatabaseType) -> &'static str {
    match database_type {
        DatabaseType::SQLite => "EXPLAIN ANALYZE is not supported for SQLite",
        DatabaseType::PostgreSQL => "EXPLAIN ANALYZE is unavailable for this statement",
    }
}

pub(super) fn mark_explain_unsupported_query(state: &mut AppState, content: &str) {
    let database_type = state.session.active_database_type_or_default();
    let message = match database_type {
        DatabaseType::SQLite => explain_unsupported_sqlite_query_message(content),
        DatabaseType::PostgreSQL => explain_unsupported_query_message(database_type),
    };
    show_explain_error_on_plan(state, message);
}

pub(super) fn mark_explain_unsupported_analyze(state: &mut AppState) {
    let database_type = state.session.active_database_type_or_default();
    show_explain_error_on_plan(state, explain_unsupported_analyze_message(database_type));
}

pub(super) fn finish_explain_unsupported_analyze(state: &mut AppState) {
    if state.session.active_db_capabilities().supports_explain() {
        mark_explain_unsupported_analyze(state);
    } else {
        mark_explain_unavailable(state);
    }
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingAnalyzeHigh { .. } | SqlModalStatus::ConfirmingAnalyzeRisk { .. }
    ) {
        state.sql_modal.cancel_confirmation();
    }
}

pub(super) fn is_multi_statement(database_type: DatabaseType, content: &str) -> bool {
    split_statements_for_database(database_type, content).len() > 1
}

pub(super) fn mark_explain_unavailable(state: &mut AppState) {
    state
        .explain
        .set_error("EXPLAIN is unavailable for this database".to_string());
    let tab = state
        .session
        .active_db_capabilities()
        .normalize_sql_modal_tab(state.sql_modal.active_tab());
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

pub(super) fn reject_unsupported_explain_analyze(state: &mut AppState) -> bool {
    if state
        .session
        .active_db_capabilities()
        .supports_explain_analyze()
    {
        return false;
    }

    if state.session.active_db_capabilities().supports_explain() {
        mark_explain_unsupported_analyze(state);
    } else {
        mark_explain_unavailable(state);
    }
    true
}

pub(super) fn reject_unsupported_explain(state: &mut AppState) -> bool {
    if state.session.active_db_capabilities().supports_explain() {
        return false;
    }

    mark_explain_unavailable(state);
    true
}
