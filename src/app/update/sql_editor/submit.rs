use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::policy::sql::statement_classifier::{self, StatementKind};
use crate::policy::write::sql_risk::{
    ConfirmationType, MultiStatementDecision, evaluate_multi_statement,
};
use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel, evaluate_sql_risk};
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::start_adhoc_if_connected;

fn multi_statement_label(sql: &str) -> &'static str {
    use crate::policy::write::sql_risk::split_statements;
    let mut worst_level = RiskLevel::Low;
    let mut worst_label = "SQL";
    for stmt in split_statements(sql) {
        let kind = statement_classifier::classify(&stmt);
        let d = evaluate_sql_risk(&kind);
        if d.risk_level > worst_level || (d.risk_level == worst_level && d.label != "SQL") {
            worst_level = d.risk_level;
            worst_label = d.label;
        }
    }
    worst_label
}
pub(super) fn reduce_submit(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::SqlModalSubmit => {
            let query = state.sql_modal.editor.content().trim().to_string();
            if query.is_empty() {
                return DispatchResult::handled();
            }
            state.sql_modal.dismiss_completion();

            match evaluate_multi_statement(&query) {
                MultiStatementDecision::Block { reason } => {
                    state.sql_modal.finish_adhoc_error(reason);
                    DispatchResult::handled()
                }
                MultiStatementDecision::Allow {
                    risk,
                    ref statements,
                } => {
                    let label = multi_statement_label(&query);
                    let decision = AdhocRiskDecision {
                        risk_level: risk.risk_level,
                        label,
                    };
                    // In read-only mode, block if any statement is a write operation
                    let has_write = statements.iter().any(|s| {
                        let kind = statement_classifier::classify(s);
                        !matches!(kind, StatementKind::Select | StatementKind::Transaction)
                    });
                    if state.session.read_only && has_write {
                        state.sql_modal.finish_adhoc_error(
                            "Read-only mode: write operations are disabled".to_string(),
                        );
                        return DispatchResult::handled();
                    }
                    match risk.confirmation {
                        ConfirmationType::Immediate => start_adhoc_if_connected(state, query),
                        ConfirmationType::TableNameInput { target } => {
                            state
                                .sql_modal
                                .begin_confirming_high(decision, Some(target));
                            DispatchResult::handled()
                        }
                    }
                }
            }
        }
        _ => DispatchResult::pass(),
    }
}
