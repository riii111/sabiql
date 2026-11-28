use crate::update::sql_editor::completion::completion_request;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{Table, TableSummary};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::table_prefetch::FailedPrefetchEntry;
use crate::ports::outbound::DbOperationError;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::check_er_completion;

const BASE_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 4;
const MAX_CONCURRENT_PREFETCH: usize = 4;
pub(super) use crate::model::table_prefetch::MAX_PREFETCH_RETRIES;

struct PrefetchStep {
    effects: Vec<Effect>,
    cache_hit: bool,
}

impl PrefetchStep {
    fn effects(effects: Vec<Effect>) -> Self {
        Self {
            effects,
            cache_hit: false,
        }
    }
}

pub(super) fn backoff_secs_for(retry_count: u32) -> u64 {
    (BASE_BACKOFF_SECS * 2u64.pow(retry_count)).min(MAX_BACKOFF_SECS)
}

#[derive(Default)]
struct PrefetchBatch {
    effects: Vec<Effect>,
    cache_hits: usize,
}

impl PrefetchBatch {
    fn push(&mut self, step: PrefetchStep) {
        self.cache_hits += usize::from(step.cache_hit);
        self.effects.extend(step.effects);
    }
}

fn finish_prefetch_batch(
    state: &mut AppState,
    run_id: u64,
    batch: PrefetchBatch,
    now: Instant,
) -> Vec<Effect> {
    let PrefetchBatch {
        mut effects,
        cache_hits,
    } = batch;
    if cache_hits == 0 {
        return effects;
    }

    // Cached tables finish without a fetch, so no fetch completion schedules the rest of the queue.
    // One schedule per batch keeps the queue moving without multiplying ProcessPrefetchQueue messages.
    let already_scheduled = effects.iter().any(|effect| {
        matches!(effect, Effect::SchedulePrefetchQueueProcessing { run_id: id } if *id == run_id)
    });
    if !already_scheduled
        && state.table_prefetch.has_pending_prefetch()
        && state.table_prefetch.prefetch_in_flight_count() < MAX_CONCURRENT_PREFETCH
    {
        effects.push(Effect::SchedulePrefetchQueueProcessing { run_id });
    }
    effects.extend(after_table_detail_cached(state, now));
    effects
}

fn after_table_detail_cached(state: &mut AppState, now: Instant) -> Vec<Effect> {
    if state.table_prefetch.prefetch_tracks_er() {
        check_er_completion(state)
    } else if state.modal.active_mode() == InputMode::SqlModal {
        completion_request(state, now)
    } else {
        vec![]
    }
}

fn prefetch_table_detail_step(
    state: &mut AppState,
    run_id: u64,
    schema: &str,
    table: &str,
    now: Instant,
) -> PrefetchStep {
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return PrefetchStep::effects(vec![]);
    }
    let qualified_name = format!("{schema}.{table}");

    if state.table_prefetch.is_table_prefetching(&qualified_name) {
        return PrefetchStep::effects(vec![]);
    }

    if let Some(entry) = state.table_prefetch.failed_prefetch(&qualified_name) {
        if entry.retry_count >= MAX_PREFETCH_RETRIES {
            let mut effects = if state.table_prefetch.prefetch_tracks_er() {
                check_er_completion(state)
            } else {
                Vec::new()
            };
            if effects.is_empty()
                && state.table_prefetch.prefetch_tracks_er()
                && state.er_preparation.is_waiting()
            {
                effects.push(Effect::SchedulePrefetchQueueProcessing { run_id });
            }
            return PrefetchStep::effects(effects);
        }

        let backoff_secs = backoff_secs_for(entry.retry_count);
        let elapsed = now.saturating_duration_since(entry.failed_at).as_secs();
        if elapsed < backoff_secs {
            let remaining = backoff_secs - elapsed;
            state.table_prefetch.queue_table_prefetch(qualified_name);
            return PrefetchStep::effects(vec![Effect::DelayedProcessPrefetchQueue {
                run_id,
                delay_secs: remaining,
            }]);
        }
    }

    let Some(endpoint) = state.session.endpoint().cloned() else {
        state.table_prefetch.defer_table_prefetch(qualified_name);
        return PrefetchStep::effects(vec![]);
    };

    if state.table_detail_cache.contains(&qualified_name) {
        state
            .table_prefetch
            .complete_table_prefetch(&qualified_name);
        return PrefetchStep {
            effects: vec![],
            cache_hit: true,
        };
    }

    state.table_prefetch.start_table_prefetch(qualified_name);

    PrefetchStep::effects(vec![Effect::PrefetchTableColumnsAndFks {
        endpoint,
        run_id,
        schema: schema.to_string(),
        table: table.to_string(),
    }])
}

pub(in crate::update) fn start_er_prefetch_all(state: &mut AppState) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if (state.table_prefetch.active_prefetch_run_id().is_none()
        || !state.table_prefetch.prefetch_tracks_er())
        && let Some(metadata) = state.session.metadata()
    {
        let run_id = state.table_prefetch.begin_er_prefetch();
        let qualified_names: Vec<String> = metadata
            .table_summaries
            .iter()
            .map(TableSummary::qualified_name)
            .collect();
        state
            .er_preparation
            .begin_all_prefetch(qualified_names.iter().cloned());

        state
            .table_detail_cache
            .resize_for_table_count(metadata.table_summaries.len());

        for qualified_name in qualified_names {
            state.table_prefetch.queue_table_prefetch(qualified_name);
        }
        vec![Effect::SchedulePrefetchQueueProcessing { run_id }]
    } else {
        vec![]
    }
}

pub(in crate::update) fn start_er_prefetch_scoped(
    state: &mut AppState,
    tables: Vec<String>,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if state.table_prefetch.active_prefetch_run_id().is_some()
        && state.table_prefetch.prefetch_tracks_er()
    {
        vec![]
    } else {
        let run_id = state.table_prefetch.begin_er_prefetch();
        state.er_preparation.begin_scoped_prefetch(&tables);

        for qualified_name in &tables {
            state
                .table_prefetch
                .queue_table_prefetch(qualified_name.clone());
        }
        if let Some(metadata) = state.session.metadata() {
            state
                .table_detail_cache
                .resize_for_table_count(metadata.table_summaries.len());
        }
        vec![Effect::SchedulePrefetchQueueProcessing { run_id }]
    }
}

pub(in crate::update) fn start_completion_prefetch(
    state: &mut AppState,
    tables: Vec<String>,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if tables.is_empty() {
        return vec![];
    }

    let run_id = if let Some(run_id) = state.table_prefetch.active_prefetch_run_id() {
        run_id
    } else {
        state.table_prefetch.begin_completion_prefetch()
    };
    for qualified_name in &tables {
        state
            .table_prefetch
            .queue_table_prefetch(qualified_name.clone());
    }
    vec![Effect::SchedulePrefetchQueueProcessing { run_id }]
}

pub(in crate::update) fn process_prefetch_queue(
    state: &mut AppState,
    run_id: u64,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return vec![];
    }
    let current_in_flight = state.table_prefetch.prefetch_in_flight_count();
    let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);
    let batch_size = available_slots.min(state.table_prefetch.pending_prefetch_count());
    let mut processed_tables = HashSet::with_capacity(batch_size);

    let mut batch = PrefetchBatch::default();
    for _ in 0..batch_size {
        let Some(qualified_name) = state.table_prefetch.take_next_prefetch() else {
            break;
        };
        if !processed_tables.insert(qualified_name.clone()) {
            state.table_prefetch.defer_table_prefetch(qualified_name);
            break;
        }
        let Some((schema, table)) = qualified_name.split_once('.') else {
            continue;
        };
        batch.push(prefetch_table_detail_step(
            state, run_id, schema, table, now,
        ));
    }

    finish_prefetch_batch(state, run_id, batch, now)
}

pub(in crate::update) fn prefetch_table_detail(
    state: &mut AppState,
    run_id: u64,
    schema: &str,
    table: &str,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    let mut batch = PrefetchBatch::default();
    batch.push(prefetch_table_detail_step(
        state, run_id, schema, table, now,
    ));
    finish_prefetch_batch(state, run_id, batch, now)
}

pub(in crate::update) fn table_detail_cached(
    state: &mut AppState,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    schema: String,
    table: String,
    outcome: Result<Box<Table>, DbOperationError>,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    if !state.session.endpoint_matches(&endpoint)
        || !state.table_prefetch.is_current_prefetch_run(run_id)
    {
        return vec![];
    }
    let qualified_name = format!("{schema}.{table}");
    let detail = match outcome {
        Ok(detail) => detail,
        Err(error) => return table_detail_cache_failed(state, run_id, qualified_name, &error, now),
    };
    state
        .table_prefetch
        .complete_table_prefetch(&qualified_name);

    state
        .table_detail_cache
        .insert(qualified_name, detail.as_ref().clone());

    let mut effects = Vec::new();

    if state.table_prefetch.has_pending_prefetch() {
        effects.push(Effect::SchedulePrefetchQueueProcessing { run_id });
    }
    effects.extend(after_table_detail_cached(state, now));

    effects
}

fn table_detail_cache_failed(
    state: &mut AppState,
    run_id: u64,
    qualified_name: String,
    error: &DbOperationError,
    now: Instant,
) -> Vec<Effect> {
    let prev_count = state
        .table_prefetch
        .failed_prefetch(&qualified_name)
        .map_or(0, |e| e.retry_count);
    let had_other_pending_before_requeue = state.table_prefetch.retry_table_prefetch(
        qualified_name,
        FailedPrefetchEntry {
            failed_at: now,
            error: error.user_message(),
            retry_count: prev_count + 1,
        },
    );
    let mut effects = Vec::new();

    if had_other_pending_before_requeue {
        effects.push(Effect::SchedulePrefetchQueueProcessing { run_id });
    }
    effects.push(Effect::DelayedProcessPrefetchQueue {
        run_id,
        delay_secs: backoff_secs_for(prev_count + 1),
    });

    if state.table_prefetch.prefetch_tracks_er() {
        effects.extend(check_er_completion(state));
    }

    effects
}
