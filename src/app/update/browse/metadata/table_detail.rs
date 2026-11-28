use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::domain::Table;
use crate::domain::connection::ConnectionEndpoint;
use crate::model::app_state::AppState;
use crate::ports::outbound::DbOperationError;

pub(in crate::update) fn table_detail_loaded(
    state: &mut AppState,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    outcome: Result<Box<Table>, DbOperationError>,
    generation: u64,
) -> Vec<Effect> {
    if !state.session.endpoint_matches(&endpoint)
        || !state.session.is_current_table_detail_run(run_id)
    {
        return vec![];
    }

    match outcome {
        Ok(detail) => {
            if state.session.set_table_detail(*detail, generation) {
                state.ui.inspector_scroll_offset = 0;
            }
        }
        Err(error) => {
            let message = error.user_message();
            if state
                .session
                .mark_table_detail_failed(generation, message.clone())
            {
                state.messages.set_error(message);
            }
        }
    }
    vec![]
}
