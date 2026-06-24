use std::time::Instant;

use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::policy::sql::statement_classifier;
use crate::policy::write::sql_risk::{
    ConfirmationType, MultiStatementDecision, evaluate_multi_statement_for_database,
    split_statements_for_database, sqlite_specific_label,
};
use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel, evaluate_sql_risk};
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::start_adhoc_if_connected;

fn multi_statement_label(database_type: DatabaseType, sql: &str) -> &'static str {
    let mut worst_level = RiskLevel::Low;
    let mut worst_label = "SQL";
    for stmt in split_statements_for_database(database_type, sql) {
        let sqlite_label = (database_type == DatabaseType::SQLite)
            .then(|| sqlite_specific_label(&stmt))
            .flatten();
        let kind = statement_classifier::classify(&stmt);
        let d = evaluate_sql_risk(&kind);
        let label = sqlite_label.unwrap_or(d.label);
        if d.risk_level > worst_level || (d.risk_level == worst_level && label != "SQL") {
            worst_level = d.risk_level;
            worst_label = label;
        }
    }
    worst_label
}
pub(super) fn reduce_submit(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::SqlModalSubmit => {
            let query = state.sql_modal.editor.content().trim().to_string();
            if query.is_empty() {
                return DispatchResult::handled();
            }
            state.sql_modal.dismiss_completion();

            let database_type = state.session.active_database_type_or_default();

            match evaluate_multi_statement_for_database(database_type, &query) {
                MultiStatementDecision::Block { reason } => {
                    state.sql_modal.finish_adhoc_error(reason);
                    DispatchResult::handled()
                }
                MultiStatementDecision::Allow { risk, .. } => {
                    let label = multi_statement_label(database_type, &query);
                    let decision = AdhocRiskDecision {
                        risk_level: risk.risk_level,
                        label,
                    };
                    if state.session.is_read_only() && !risk.read_only_allowed {
                        state.sql_modal.finish_adhoc_error(
                            "Read-only mode: write operations are disabled".to_string(),
                        );
                        return DispatchResult::handled();
                    }
                    match risk.confirmation {
                        ConfirmationType::Immediate => start_adhoc_if_connected(state, query, now),
                        ConfirmationType::Acknowledge { reason, label } => {
                            state.sql_modal.begin_confirming_risk(reason, label);
                            DispatchResult::handled()
                        }
                        ConfirmationType::TableNameInput { target } => {
                            state.sql_modal.begin_confirming_high(decision, target);
                            DispatchResult::handled()
                        }
                    }
                }
            }
        }
        _ => DispatchResult::pass(),
    }
}
