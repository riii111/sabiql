use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::policy::write::sql_risk::{
    ConfirmationType, MultiStatementDecision, adhoc_label_for_table_name_confirmation,
    evaluate_multi_statement_for_database,
};
use crate::policy::write::write_guardrails::AdhocRiskDecision;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::start_adhoc_if_connected;

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
                            let label = adhoc_label_for_table_name_confirmation(
                                database_type,
                                &query,
                            )
                            .expect("TableNameInput confirmation must have a matching statement");
                            let decision = AdhocRiskDecision {
                                risk_level: risk.risk_level,
                                label,
                            };
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
