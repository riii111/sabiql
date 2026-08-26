use std::time::Instant;

use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::policy::write::sql_risk::{
    ConfirmationType, MultiStatementDecision, SqlRiskDecision,
    evaluate_multi_statement_for_database_with_context,
    evaluate_mysql_multi_statement_with_lower_case_table_names,
};
use crate::policy::write::write_guardrails::AdhocRiskDecision;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::helpers::start_adhoc_if_connected;

fn into_submit_result<Statement>(
    decision: MultiStatementDecision<Statement>,
) -> Result<SqlRiskDecision, String> {
    match decision {
        MultiStatementDecision::Block { reason } => Err(reason),
        MultiStatementDecision::Allow { risk, .. } => Ok(risk),
    }
}

pub(super) fn reduce_submit(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::SqlModalSubmit => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            let query = state.sql_modal.editor.content().trim().to_string();
            if query.is_empty() {
                return DispatchResult::handled();
            }
            state.sql_modal.dismiss_completion();

            let database_type = state.session.active_database_type_or_default();

            let decision = if database_type == DatabaseType::MySQL {
                into_submit_result(evaluate_mysql_multi_statement_with_lower_case_table_names(
                    &query,
                    state.session.active_database(),
                    state.session.mysql_lower_case_table_names(),
                ))
            } else {
                into_submit_result(evaluate_multi_statement_for_database_with_context(
                    database_type,
                    state.session.active_database(),
                    &query,
                ))
            };

            match decision {
                Err(reason) => {
                    state.sql_modal.finish_adhoc_error(reason);
                    DispatchResult::handled()
                }
                Ok(risk) => handle_allowed_query(state, query, now, risk),
            }
        }
        _ => DispatchResult::pass(),
    }
}

fn handle_allowed_query(
    state: &mut AppState,
    query: String,
    now: Instant,
    risk: SqlRiskDecision,
) -> DispatchResult {
    if state.session.is_read_only() && !risk.read_only_allowed {
        state
            .sql_modal
            .finish_adhoc_error("Read-only mode: write operations are disabled".to_string());
        return DispatchResult::handled();
    }

    match risk.confirmation {
        ConfirmationType::Immediate => start_adhoc_if_connected(state, query, now),
        ConfirmationType::Acknowledge { reason, label } => {
            state.sql_modal.begin_confirming_risk(reason, label);
            DispatchResult::handled()
        }
        ConfirmationType::TableNameInput { target, label } => {
            let decision = AdhocRiskDecision {
                risk_level: risk.risk_level,
                label,
            };
            state.sql_modal.begin_confirming_high(decision, target);
            DispatchResult::handled()
        }
    }
}
