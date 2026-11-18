use std::collections::HashSet;

use crate::cmd::effect::Effect;
use crate::domain::ErTableInfo;
use crate::domain::er::fk_neighbors_of_seeds;
use crate::model::app_state::AppState;
use crate::model::table_detail_cache::TableDetailCache;

use super::check_er_completion;

pub(super) fn expand_prefetch_with_fk_neighbors(state: &mut AppState, run_id: u64) -> Vec<Effect> {
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return vec![];
    }
    let neighbors = discover_fk_neighbors(
        &state.table_detail_cache,
        state.er_preparation.seed_tables(),
    );
    queue_fk_neighbors(state, run_id, &neighbors)
}

pub(super) fn queue_fk_neighbors(
    state: &mut AppState,
    run_id: u64,
    tables: &[String],
) -> Vec<Effect> {
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return vec![];
    }
    state.er_preparation.mark_fk_expanded();

    if tables.is_empty() {
        return check_er_completion(state);
    }

    for qualified_name in tables {
        state
            .table_prefetch
            .queue_pending_table(qualified_name.clone());
    }
    vec![Effect::SchedulePrefetchQueueProcessing { run_id }]
}

fn discover_fk_neighbors(
    table_detail_cache: &TableDetailCache,
    seed_tables: &[String],
) -> Vec<String> {
    let seed_set: HashSet<&str> = seed_tables.iter().map(String::as_str).collect();
    let cached_seeds: Vec<ErTableInfo> = table_detail_cache
        .iter()
        .filter(|(name, _)| seed_set.contains(name.as_str()))
        .map(|(name, table)| ErTableInfo::from_table(name, table))
        .collect();
    let cached_names: HashSet<String> = table_detail_cache
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    fk_neighbors_of_seeds(&cached_seeds, &seed_set, &cached_names)
}
