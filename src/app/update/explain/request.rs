use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::sql::mysql_statement::{
    MysqlStatementKind, classify_mysql_statement, mysql_explain_rejection_message,
};
use crate::policy::write::sql_risk::evaluate_mysql_explain_target;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::ports::outbound::AccessMode;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    begin_explain_running, is_multi_statement, mark_explain_unavailable,
    mark_explain_unsupported_query, show_explain_error_on_plan,
};

pub(super) fn reduce_request(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::ExplainRequest => {
            let content = state.sql_modal.editor.content().trim().to_string();
            if content.is_empty() {
                return DispatchResult::handled();
            }
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };
            if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
                return DispatchResult::handled();
            }
            let database_type = state.session.active_database_type_or_default();
            if database_type == DatabaseType::MySQL {
                if let Some(message) = mysql_explain_rejection_message(&content) {
                    show_explain_error_on_plan(state, message);
                    return DispatchResult::handled();
                }
            } else if is_multi_statement(database_type, &content) {
                show_explain_error_on_plan(state, "EXPLAIN does not support multiple statements");
                return DispatchResult::handled();
            }
            let mysql_explain_dml = database_type == DatabaseType::MySQL
                && classify_mysql_statement(&content).is_ok_and(|statement| {
                    matches!(
                        statement.kind,
                        MysqlStatementKind::Insert
                            | MysqlStatementKind::Replace
                            | MysqlStatementKind::Update { .. }
                            | MysqlStatementKind::Delete { .. }
                    )
                });
            if database_type == DatabaseType::MySQL
                && !mysql_explain_dml
                && evaluate_mysql_explain_target(&content, false).is_none()
            {
                show_explain_error_on_plan(
                    state,
                    "MySQL EXPLAIN only supports side-effect-free read statements",
                );
                return DispatchResult::handled();
            }

            let query = match services
                .sql_dialect
                .build_explain_sql(database_type, &content)
            {
                Some(query) => query,
                None if FeaturePolicy::new(state.session.active_engine_feature_profile())
                    .is_enabled(FeatureRequirement::Explain) =>
                {
                    mark_explain_unsupported_query(state, &content);
                    return DispatchResult::handled();
                }
                None => {
                    mark_explain_unavailable(state);
                    return DispatchResult::handled();
                }
            };
            let run_id = begin_explain_running(state, now);
            let database_generation = state.session.database_generation();

            DispatchResult::handled_with(vec![Effect::ExecuteExplain {
                dsn,
                database_type,
                database_generation,
                run_id,
                query,
                source_query: content,
                is_analyze: false,
                access_mode: AccessMode::ReadOnly,
            }])
        }
        _ => DispatchResult::pass(),
    }
}
