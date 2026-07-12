use crate::cmd::effect::Effect;
use crate::domain::connection::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::connection::cache::ConnectionCache;
use crate::update::action::ConnectionTarget;
use crate::update::query_context::termination_effects;

fn reset_sql_and_er_state(state: &mut AppState) {
    state.sql_modal.reset_prefetch();
    state.er_preparation.reset();
    state.ui.reset_er_picker_request();
}

pub(super) fn reset_for_new_connection(
    state: &mut AppState,
    id: &ConnectionId,
    dsn: &str,
    name: &str,
    database_type: DatabaseType,
) {
    reset_active_connection_state(state);
    state
        .session
        .activate_connection_with_dsn(id, name, database_type, dsn);
}

pub(super) fn connection_save_fetch_effects(
    state: &AppState,
    dsn: &str,
    run_id: u64,
    database_type: DatabaseType,
) -> Vec<Effect> {
    let fetch = Effect::FetchMetadata {
        dsn: dsn.to_string(),
        run_id,
    };
    if database_type == DatabaseType::SQLite {
        vec![Effect::Sequence(termination_effects(
            &state.query,
            vec![
                Effect::CacheInvalidate {
                    dsn: dsn.to_string(),
                },
                Effect::ClearCompletionEngineCache,
                fetch,
            ],
        ))]
    } else {
        termination_effects(
            &state.query,
            vec![Effect::ClearCompletionEngineCache, fetch],
        )
    }
}

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected(),
        state.ui.inspector_tab(),
        state.query.current_result().cloned(),
        state.query.result_history().clone(),
    )
}

pub(super) fn reset_active_connection_state(state: &mut AppState) {
    state.session.reset(&mut state.query);
    state.result_interaction.reset_view();
    state.ui.set_explorer_selection(None);
    reset_sql_and_er_state(state);
}

pub(super) fn restore_cache(
    state: &mut AppState,
    cache: &ConnectionCache,
    target: &ConnectionTarget,
) {
    state.session.restore_from_cache_for_connection(
        cache,
        &mut state.query,
        &target.id,
        &target.name,
        target.database_type,
        &target.dsn,
    );
    state.ui.set_inspector_tab(
        state
            .session
            .active_db_capabilities()
            .normalize_inspector_tab(cache.inspector_tab),
    );
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.result_interaction.reset_view();
    reset_sql_and_er_state(state);
}
