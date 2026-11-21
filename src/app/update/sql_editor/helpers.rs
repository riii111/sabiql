use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::ports::outbound::AccessMode;

pub(super) fn start_adhoc_if_connected(
    state: &mut AppState,
    query: String,
    now: Instant,
) -> Vec<Effect> {
    let Some(endpoint) = state.session.endpoint().cloned() else {
        state
            .sql_modal
            .finish_adhoc_error("No active connection".to_string());
        return vec![];
    };

    let run_id = state.query.begin_non_preview_running(now);
    state.sql_modal.begin_adhoc_running();
    vec![Effect::ExecuteAdhoc {
        endpoint,
        run_id,
        query,
        access_mode: AccessMode::from_read_only(state.session.is_read_only()),
    }]
}
