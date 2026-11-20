use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::ErTableInfo;
use crate::domain::er::{er_output_filename, fk_reachable_tables_multi};
use crate::model::app_state::AppState;
use crate::update::action::ErDiagramInfo;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(in crate::update) fn er_diagram_opened(
    state: &mut AppState,
    info: ErDiagramInfo,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let ErDiagramInfo {
        run_id,
        path,
        table_count,
        total_tables,
    } = info;
    if !state.er_preparation.is_current_run(run_id) {
        return vec![];
    }
    state.er_preparation.mark_idle();
    // Reset so next ErOpenDiagram re-evaluates target_tables from scratch.
    state.table_prefetch.invalidate_prefetch();
    state.messages.set_success_at(
        format!("✓ Opened {path} ({table_count}/{total_tables} tables) — Stale? Press r to reload"),
        now,
    );
    vec![]
}

pub(in crate::update) fn er_diagram_failed(
    state: &mut AppState,
    run_id: u64,
    error: String,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if !state.er_preparation.is_current_run(run_id) {
        return vec![];
    }
    state.er_preparation.mark_idle();
    state.messages.set_error(error);
    vec![]
}

pub(in crate::update) fn er_log_write_failed(state: &mut AppState, error: String) -> Vec<Effect> {
    state.messages.set_error(error);
    vec![]
}

pub(in crate::update) fn er_open_diagram(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if state.er_preparation.is_busy() {
        return vec![];
    }

    let Some(dsn) = state.session.dsn().map(String::from) else {
        state.messages.set_error("No active connection".to_string());
        return vec![];
    };
    if state.session.metadata().is_none() {
        state
            .messages
            .set_error("Metadata not loaded yet".to_string());
        return vec![];
    }

    state.table_prefetch.invalidate_prefetch();
    let run_id = state.er_preparation.start_waiting_run();
    state
        .messages
        .set_success_at("Checking for schema changes...".to_string(), now);

    vec![Effect::SmartErRefresh { dsn, run_id }]
}

pub(in crate::update) fn er_generate_from_cache(state: &mut AppState) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    generate_from_cache(state)
}

pub(super) fn generate_from_cache(state: &mut AppState) -> Vec<Effect> {
    if !state.er_preparation.can_generate_from_cache() {
        return vec![];
    }

    state.er_preparation.mark_rendering();
    let run_id = state.er_preparation.run_id();
    let total_tables = state
        .session
        .metadata()
        .map_or(0, |m| m.table_summaries.len());

    let cached_tables: Vec<ErTableInfo> = state
        .table_detail_cache
        .iter()
        .map(|(name, table)| ErTableInfo::from_table(name, table))
        .collect();
    if cached_tables.is_empty() {
        return fail_generation(state, "No table data loaded yet");
    }

    let target_tables = state.er_preparation.target_tables();
    let filename = er_output_filename(target_tables, cached_tables.len());
    let tables = if target_tables.is_empty() || target_tables.len() == cached_tables.len() {
        cached_tables
    } else {
        fk_reachable_tables_multi(&cached_tables, target_tables, 1)
    };
    if tables.is_empty() {
        return fail_generation(state, "Selected tables not found in cached data");
    }

    vec![Effect::GenerateErDiagram {
        run_id,
        total_tables,
        project_name: state.project_name().to_string(),
        tables,
        filename,
    }]
}

fn fail_generation(state: &mut AppState, error: &str) -> Vec<Effect> {
    state.er_preparation.mark_idle();
    state.messages.set_error(error.to_string());
    vec![]
}
