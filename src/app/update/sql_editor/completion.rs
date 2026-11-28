use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::policy::sql::completion::{CompletionEngine, CompletionInput};
use crate::update::browse::metadata::prefetch::start_completion_prefetch;

pub(in crate::update) fn completion_request(state: &mut AppState, _now: Instant) -> Vec<Effect> {
    let outcome = CompletionEngine::new(&state.table_detail_cache).complete(CompletionInput {
        content: state.sql_modal.editor().content(),
        cursor: state.sql_modal.editor().cursor(),
        database_type: state.session.active_database_type_or_default(),
        active_database: state.session.active_database(),
        metadata: state.session.metadata(),
        table_detail: state.session.table_detail(),
    });
    state.sql_modal.apply_completion_update(
        &outcome.candidates,
        outcome.trigger_position,
        outcome.visible,
    );
    start_completion_prefetch(state, outcome.missing_tables)
}
