use crate::cmd::effect::Effect;
use std::sync::Arc;

use crate::domain::DatabaseMetadata;
use crate::model::app_state::AppState;
use crate::model::connection::cache::ConnectionCache;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::update::action::{Action, ConnectionTarget};
use crate::update::query_context::termination_effects;

fn reset_connection_scoped_state(state: &mut AppState) {
    state.query_history_picker.reset();
    state.sql_modal.reset_completion();
    state.table_prefetch.reset_prefetch();
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

pub(super) fn reset_for_new_connection(state: &mut AppState, target: &ConnectionTarget) {
    let inspector_tab = state.ui.inspector_tab();
    let sql_modal_tab = state.sql_modal.active_tab();
    reset_state_before_connection_reconciliation(state);
    state.ui.set_inspector_tab(inspector_tab);
    state.sql_modal.set_active_tab(sql_modal_tab);
    state.session.activate_connection_with_target(
        &target.id,
        &target.name,
        target.database_type,
        &target.dsn,
        target.database.as_deref(),
    );
    reconcile_connection_state(state, inspector_tab);
}

pub(super) fn connection_save_fetch_effects(
    state: &AppState,
    dsn: &str,
    run_id: u64,
    metadata: Option<Arc<DatabaseMetadata>>,
) -> Vec<Effect> {
    let metadata_effect = if let Some(metadata) = metadata {
        Effect::DispatchActions(vec![Action::MetadataLoaded { run_id, metadata }])
    } else {
        Effect::FetchMetadata {
            dsn: dsn.to_string(),
            run_id,
        }
    };

    termination_effects(
        &state.query,
        vec![Effect::ClearCompletionEngineCache, metadata_effect],
    )
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

pub(super) fn save_current_connection_cache(state: &mut AppState) {
    let Some(current_id) = state.session.active_connection_id().cloned() else {
        return;
    };

    let cache = state.session.to_cache(
        state.ui.explorer_selected(),
        state.ui.inspector_tab(),
        state.query.current_result().cloned(),
        state.query.pagination.clone(),
    );
    state.connection_caches.insert(current_id, cache);
}

pub(super) fn cancel_connection_task_effects(state: &mut AppState) -> Vec<Effect> {
    let had_pending_probe = state.session.pending_mysql_connection_probe().is_some();
    let table_detail_retry =
        state
            .session
            .retry_table_detail_after_probe_failure()
            .map(|(dsn, generation, run_id)| Effect::FetchTableDetail {
                dsn,
                schema: state.query.pagination.schema().to_string(),
                table: state.query.pagination.table().to_string(),
                generation,
                run_id,
            });
    state.session.clear_mysql_connection_probe();
    if had_pending_probe {
        state.table_prefetch.reset_prefetch();
        state.er_preparation.reset();
    }

    let mut effects = vec![Effect::CancelConnectionTask];
    if let Some(table_detail_retry) = table_detail_retry {
        effects.push(table_detail_retry);
    }
    effects
}

pub(super) fn reset_active_connection_state(state: &mut AppState) {
    let inspector_tab = state.ui.inspector_tab();
    reset_state_before_connection_reconciliation(state);
    reconcile_connection_state(state, inspector_tab);
}

fn reset_state_before_connection_reconciliation(state: &mut AppState) {
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
