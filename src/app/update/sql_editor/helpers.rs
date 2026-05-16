use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn start_adhoc_if_connected(state: &mut AppState, query: String) -> DispatchResult {
    let Some(dsn) = state.session.dsn.clone() else {
        state
            .sql_modal
            .finish_adhoc_error("No active connection".to_string());
        return DispatchResult::handled();
    };

    state.sql_modal.begin_adhoc_running();
    DispatchResult::handled_with(vec![Effect::ExecuteAdhoc {
        dsn,
        query,
        read_only: state.session.read_only,
    }])
}
