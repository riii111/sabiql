use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::{DatabaseType, mysql_sql::mysql_explain_rejection_message};
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::ports::outbound::AccessMode;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::helpers::{
    begin_explain_running, is_multi_statement, mark_explain_unavailable,
    mark_explain_unsupported_query, show_explain_error_on_plan,
};

pub(in crate::update) fn explain_request(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let content = state.sql_modal.editor.content().trim().to_string();
    if content.is_empty() {
        return vec![];
    }
    let Some(dsn) = state.session.dsn().map(String::from) else {
        return vec![];
    };
    if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
        return vec![];
    }
    let database_type = state.session.active_database_type_or_default();
    if database_type == DatabaseType::MySQL {
        if let Some(message) = mysql_explain_rejection_message(&content) {
            show_explain_error_on_plan(state, message);
            return vec![];
        }
    } else if is_multi_statement(database_type, &content) {
        show_explain_error_on_plan(state, "EXPLAIN does not support multiple statements");
        return vec![];
    }
    let query = match database_type.dialect().explain_sql(&content) {
        Some(query) => query,
        None if FeaturePolicy::new(&state.session.active_engine_feature_profile())
            .is_enabled(FeatureRequirement::Explain) =>
        {
            mark_explain_unsupported_query(state, &content);
            return vec![];
        }
        None => {
            mark_explain_unavailable(state);
            return vec![];
        }
    };
    let run_id = begin_explain_running(state, now);
    let database_generation = state.session.database_generation();

    vec![Effect::ExecuteExplain {
        dsn,
        database_type,
        database_generation,
        run_id,
        query,
        source_query: content,
        is_analyze: false,
        access_mode: AccessMode::ReadOnly,
    }]
}
