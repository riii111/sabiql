use std::time::Instant;

use crate::model::app_state::AppState;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_tabs(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::CompareEditQuery => {
            let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
            if !feature_policy.is_enabled(FeatureRequirement::PlanComparison) {
                state.messages.set_error_at(
                    "Plan comparison is not available for this connection".to_string(),
                    now,
                );
                return DispatchResult::handled();
            }
            if let Some(ref right) = state.explain.right {
                let query = right.full_query.clone();
                state.sql_modal.load_query_for_editing(query);
            }
            DispatchResult::handled()
        }

        Action::SqlModalNextTab => {
            let tab = state
                .session
                .active_engine_feature_profile()
                .next_sql_modal_tab(state.sql_modal.active_tab());
            state.sql_modal.set_active_tab(tab);
            DispatchResult::handled()
        }

        Action::SqlModalPrevTab => {
            let tab = state
                .session
                .active_engine_feature_profile()
                .prev_sql_modal_tab(state.sql_modal.active_tab());
            state.sql_modal.set_active_tab(tab);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
