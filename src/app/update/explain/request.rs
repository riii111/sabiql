use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    begin_explain_running, is_multi_statement, mark_explain_unavailable,
    reject_unsupported_explain, show_explain_error_on_plan,
};

pub(super) fn reduce_request(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::ExplainRequest => {
            if reject_unsupported_explain(state, services) {
                return DispatchResult::handled();
            }
            let content = state.sql_modal.editor.content().trim().to_string();
            if content.is_empty() {
                return DispatchResult::handled();
            }
            let Some(dsn) = state.session.dsn.clone() else {
                return DispatchResult::handled();
            };
            if matches!(state.sql_modal.status(), SqlModalStatus::Running) {
                return DispatchResult::handled();
            }
            if is_multi_statement(&content) {
                show_explain_error_on_plan(state, "EXPLAIN does not support multiple statements");
                return DispatchResult::handled();
            }

            let Some(query) = services.sql_dialect.build_explain_sql(&content) else {
                mark_explain_unavailable(state, services);
                return DispatchResult::handled();
            };
            let run_id = begin_explain_running(state, now);

            DispatchResult::handled_with(vec![Effect::ExecuteExplain {
                dsn,
                run_id,
                query,
                source_query: content,
                is_analyze: false,
                read_only: true,
            }])
        }
        _ => DispatchResult::pass(),
    }
}
