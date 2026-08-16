use crate::cmd::effect::Effect;
use crate::domain::connection::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::connection::cache::ConnectionCache;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::update::action::ConnectionTarget;
use crate::update::query_context::termination_effects;

fn reset_connection_scoped_state(state: &mut AppState) {
    state.query_history_picker.reset();
    state.sql_modal.reset_completion();
    state.sql_modal.reset_prefetch();
    state.explain.reset_for_connection_change();
    state.er_preparation.reset();
    state.ui.reset_er_picker_request();
    state.ui.set_inspector_scroll_offset(0);
    state.ui.set_inspector_horizontal_offset(0);
    state.sqlite_diagnostics.clear();
}

fn reconcile_connection_state(state: &mut AppState, inspector_tab: InspectorTab) {
    let profile = state.session.active_engine_feature_profile();
    let inspector_tab = profile.normalize_inspector_tab(inspector_tab);
    let sql_modal_tab = profile.normalize_sql_modal_tab(state.sql_modal.active_tab());

    state.ui.set_inspector_tab(inspector_tab);
    state.sql_modal.set_active_tab(sql_modal_tab);
}

pub(super) fn reset_for_new_connection(
    state: &mut AppState,
    id: &ConnectionId,
    dsn: &str,
    name: &str,
    database_type: DatabaseType,
    database: Option<&str>,
) {
    let inspector_tab = state.ui.inspector_tab();
    let sql_modal_tab = state.sql_modal.active_tab();
    reset_active_connection_state_inner(state);
    state.ui.set_inspector_tab(inspector_tab);
    state.sql_modal.set_active_tab(sql_modal_tab);
    state
        .session
        .activate_connection_with_target(id, name, database_type, dsn, database);
    reconcile_connection_state(state, inspector_tab);
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

pub(super) fn mysql_connection_completion_effects(state: &mut AppState, dsn: &str) -> Vec<Effect> {
    state.session.mark_connecting();
    let run_id = state.session.begin_metadata_refresh();
    let effects = vec![
        Effect::ClearCompletionEngineCache,
        Effect::FetchMetadata {
            dsn: dsn.to_string(),
            run_id,
        },
    ];
    termination_effects(&state.query, effects)
}

pub(super) fn save_current_cache(state: &AppState) -> ConnectionCache {
    state.session.to_cache(
        state.ui.explorer_selected(),
        state.ui.inspector_tab(),
        state.query.current_result().cloned(),
        state.query.result_history().clone(),
    )
}

pub(super) fn save_current_connection_cache(state: &mut AppState) {
    let Some(current_id) = state.session.active_connection_id().cloned() else {
        return;
    };

    let cache = save_current_cache(state);
    state.connection_caches.save(&current_id, cache);
}

pub(super) fn restore_current_mysql_cache(state: &mut AppState) {
    if state.session.active_database_type() != Some(DatabaseType::MySQL) {
        return;
    }

    let Some(id) = state.session.active_connection_id().cloned() else {
        return;
    };
    let Some(dsn) = state.session.dsn().map(str::to_string) else {
        return;
    };
    let database = state.session.active_database().map(str::to_string);
    let Some(cache) = state.connection_caches.get(&id).cloned() else {
        return;
    };
    if !cache.is_valid_mysql_snapshot(&dsn, database.as_deref()) {
        return;
    }

    match &cache.query_result {
        Some(result) => state.query.set_current_result(result.clone()),
        None => state.query.clear_current_result(),
    }
    state.query.restore_history(cache.result_history.clone());
    state
        .session
        .mark_effective_user_loaded(cache.effective_user.clone());
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.ui.set_inspector_tab(cache.inspector_tab);
}

pub(super) fn reset_active_connection_state(state: &mut AppState) {
    let inspector_tab = state.ui.inspector_tab();
    reset_active_connection_state_inner(state);
    reconcile_connection_state(state, inspector_tab);
}

fn reset_active_connection_state_inner(state: &mut AppState) {
    state.session.reset(&mut state.query);
    state.result_interaction.reset_view();
    state.ui.set_explorer_selection(None);
    reset_connection_scoped_state(state);
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
        target.database.as_deref(),
    );
    reconcile_connection_state(state, cache.inspector_tab);
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
    state.result_interaction.reset_view();
    reset_connection_scoped_state(state);
}
