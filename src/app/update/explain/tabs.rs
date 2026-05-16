use std::time::Instant;

use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_tabs(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::CompareEditQuery => {
            if let Some(ref right) = state.explain.right {
                let query = right.full_query.clone();
                state.sql_modal.load_query_for_editing(query);
            }
            DispatchResult::handled()
        }

        Action::SqlModalNextTab => {
            let tab = services
                .db_capabilities
                .next_sql_modal_tab(state.sql_modal.active_tab());
            state.sql_modal.set_active_tab(tab);
            DispatchResult::handled()
        }

        Action::SqlModalPrevTab => {
            let tab = services
                .db_capabilities
                .prev_sql_modal_tab(state.sql_modal.active_tab());
            state.sql_modal.set_active_tab(tab);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
