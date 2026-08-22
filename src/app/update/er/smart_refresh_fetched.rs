use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::action::{Action, SmartErRefreshFetched};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_smart_refresh_fetched(state: &AppState, action: &Action) -> DispatchResult {
    match action {
        Action::SmartErRefreshFetched(SmartErRefreshFetched {
            dsn,
            run_id,
            new_metadata,
            signature_snapshot,
        }) => {
            if !state.session.dsn_matches(dsn) || !state.er_preparation.is_current_run(*run_id) {
                return DispatchResult::handled();
            }

            DispatchResult::handled_with(vec![Effect::SmartErRefreshCacheAndDiff {
                dsn: dsn.clone(),
                run_id: *run_id,
                new_metadata: new_metadata.clone(),
                signature_snapshot: Arc::clone(signature_snapshot),
            }])
        }
        _ => DispatchResult::pass(),
    }
}
