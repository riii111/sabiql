use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::sql::statement_classifier::{self, StatementKind};
use crate::policy::write::sql_risk::{ConfirmationType, evaluate_sql_risk};
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    begin_explain_running, is_multi_statement, mark_explain_unavailable,
    reject_unsupported_explain, show_explain_error_on_plan,
};

pub(super) fn reduce_analyze(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::ExplainAnalyzeRequest => {
            if reject_unsupported_explain(state, services) {
                return DispatchResult::handled();
            }
            let content = state.sql_modal.editor.content().trim().to_string();
            if content.is_empty() {
                return DispatchResult::handled();
            }
            let Some(dsn) = &state.session.dsn else {
                return DispatchResult::handled();
            };
            let dsn = dsn.clone();
            if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
                return DispatchResult::handled();
            }
            if is_multi_statement(&content) {
                show_explain_error_on_plan(
                    state,
                    "EXPLAIN ANALYZE does not support multiple statements",
                );
                return DispatchResult::handled();
            }
            let kind = statement_classifier::classify(&content);
            let risk = evaluate_sql_risk(&kind, &content);

            let is_dml = !matches!(kind, StatementKind::Select | StatementKind::Transaction);

            if state.session.read_only && is_dml {
                show_explain_error_on_plan(
                    state,
                    "Read-only mode: EXPLAIN ANALYZE is blocked for DML statements.",
                );
                return DispatchResult::handled();
            }

            state.explain.confirm_scroll_offset = 0;

            match risk.confirmation {
                ConfirmationType::TableNameInput { target } => {
                    state
                        .sql_modal
                        .begin_confirming_analyze_high(content, Some(target));
                }
                ConfirmationType::Immediate => {
                    let Some(explain_query) =
                        services.sql_dialect.build_explain_analyze_sql(&content)
                    else {
                        mark_explain_unavailable(state, services);
                        return DispatchResult::handled();
                    };
                    begin_explain_running(state, now);
                    return DispatchResult::handled_with(vec![Effect::ExecuteExplain {
                        dsn,
                        query: explain_query,
                        is_analyze: true,
                        read_only: state.session.read_only,
                    }]);
                }
            }

            DispatchResult::handled()
        }

        Action::ExplainAnalyzeConfirm => {
            if reject_unsupported_explain(state, services) {
                return DispatchResult::handled();
            }
            let query = match state.sql_modal.status() {
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    query,
                    input,
                    target_name,
                } => target_name
                    .as_ref()
                    .is_some_and(|name| input.content() == name)
                    .then(|| query.clone()),
                _ => None,
            };
            if let Some(query) = query
                && let Some(dsn) = state.session.dsn.clone()
            {
                let Some(explain_query) = services.sql_dialect.build_explain_analyze_sql(&query)
                else {
                    mark_explain_unavailable(state, services);
                    return DispatchResult::handled();
                };
                begin_explain_running(state, now);
                return DispatchResult::handled_with(vec![Effect::ExecuteExplain {
                    dsn,
                    query: explain_query,
                    is_analyze: true,
                    read_only: state.session.read_only,
                }]);
            }
            DispatchResult::handled()
        }

        Action::ExplainAnalyzeCancel => {
            if matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingAnalyzeHigh { .. }
            ) {
                state.sql_modal.cancel_confirmation();
            }
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
