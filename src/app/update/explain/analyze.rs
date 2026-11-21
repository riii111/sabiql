use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::domain::connection::ConnectionEndpoint;
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::sql::statement_classifier;
use crate::policy::write::sql_risk::{
    ConfirmationType, evaluate_mysql_explain_analyze_target, evaluate_sql_risk_for_database,
};
use crate::ports::outbound::AccessMode;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::helpers::{
    begin_explain_running, finish_explain_unsupported_analyze, is_multi_statement,
    show_explain_error_on_plan,
};

pub(in crate::update) fn explain_analyze_request(
    state: &mut AppState,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let content = state.sql_modal.editor.content().trim().to_string();
    if content.is_empty() {
        return vec![];
    }
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };
    if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
        return vec![];
    }
    let database_type = state.session.active_database_type_or_default();
    if database_type != DatabaseType::MySQL && is_multi_statement(database_type, &content) {
        show_explain_error_on_plan(
            state,
            "EXPLAIN ANALYZE does not support multiple statements",
        );
        return vec![];
    }
    let risk = if database_type == DatabaseType::MySQL {
        let Some(risk) = evaluate_mysql_explain_analyze_target(&content) else {
            show_explain_error_on_plan(
                state,
                "MySQL EXPLAIN ANALYZE only supports side-effect-free SELECT or TABLE statements",
            );
            return vec![];
        };
        risk
    } else {
        let kind = statement_classifier::classify(&content);
        evaluate_sql_risk_for_database(database_type, &kind, &content)
    };

    if state.session.is_read_only() && !risk.read_only_allowed {
        show_explain_error_on_plan(
            state,
            "Read-only mode: EXPLAIN ANALYZE is blocked for DML statements.",
        );
        return vec![];
    }

    state.explain.confirm_scroll_offset = 0;

    match risk.confirmation {
        ConfirmationType::TableNameInput { target, .. } => {
            state
                .sql_modal
                .begin_confirming_analyze_high(content, target);
        }
        ConfirmationType::Acknowledge { reason, .. } => {
            state
                .sql_modal
                .begin_confirming_analyze_risk(content, reason);
        }
        ConfirmationType::Immediate => {
            let Some(explain_query) = database_type.dialect().explain_analyze_sql(&content) else {
                finish_explain_unsupported_analyze(state);
                return vec![];
            };
            return start_analyze_execution(state, now, endpoint, explain_query, content);
        }
    }

    vec![]
}

pub(in crate::update) fn explain_analyze_confirm(
    state: &mut AppState,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        state.sql_modal.cancel_confirmation();
        return vec![];
    }
    let query = match state.sql_modal.status() {
        SqlModalStatus::ConfirmingAnalyzeHigh {
            query,
            input,
            target_name,
        } => (input.content() == target_name.as_str()).then(|| query.clone()),
        SqlModalStatus::ConfirmingAnalyzeRisk { query, .. } => Some(query.clone()),
        _ => None,
    };
    if let Some(query) = query
        && let Some(endpoint) = state.session.endpoint().cloned()
    {
        let database_type = state.session.active_database_type_or_default();
        if database_type == DatabaseType::MySQL
            && evaluate_mysql_explain_analyze_target(&query).is_none()
        {
            finish_explain_unsupported_analyze(state);
            return vec![];
        }
        let Some(explain_query) = database_type.dialect().explain_analyze_sql(&query) else {
            finish_explain_unsupported_analyze(state);
            return vec![];
        };
        return start_analyze_execution(state, now, endpoint, explain_query, query);
    }
    vec![]
}

pub(in crate::update) fn explain_analyze_cancel(state: &mut AppState) -> Vec<Effect> {
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingAnalyzeHigh { .. } | SqlModalStatus::ConfirmingAnalyzeRisk { .. }
    ) {
        state.sql_modal.cancel_confirmation();
    }
    vec![]
}

fn start_analyze_execution(
    state: &mut AppState,
    now: Instant,
    endpoint: Arc<ConnectionEndpoint>,
    query: String,
    source_query: String,
) -> Vec<Effect> {
    let database_type = state.session.active_database_type_or_default();
    let run_id = begin_explain_running(state, now);
    let database_generation = state.session.database_generation();
    vec![Effect::ExecuteExplain {
        endpoint,
        database_generation,
        run_id,
        query,
        source_query,
        is_analyze: true,
        access_mode: AccessMode::from_read_only(
            database_type == DatabaseType::MySQL || state.session.is_read_only(),
        ),
    }]
}
