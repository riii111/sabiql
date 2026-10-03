use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::{DatabaseType, mysql_sql::mysql_explain_rejection_message};
use crate::model::app_state::AppState;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::ports::outbound::AccessMode;
use crate::sql_builder::build_explain_sql;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    ExplainRequestInput, begin_explain_running, explain_request_input, is_multi_statement,
    mark_explain_unavailable, mark_explain_unsupported_query, show_explain_error_on_plan,
};

pub(super) fn reduce_request(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ExplainRequest => {
            let Some(ExplainRequestInput { content, dsn }) = explain_request_input(state) else {
                return DispatchResult::handled();
            };
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
            let query = match build_explain_sql(database_type, &content) {
                Some(query) => query,
                None if FeaturePolicy::new(&state.session.active_engine_feature_profile())
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
