use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::TableSummary;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::table_prefetch::FailedPrefetchEntry;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::check_er_completion;

const BASE_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 4;
const MIN_COMPLETION_CACHE_CAPACITY: usize = 500;
const MAX_COMPLETION_CACHE_CAPACITY: usize = 10_000;
pub(super) use crate::model::table_prefetch::MAX_PREFETCH_RETRIES;

pub(super) fn backoff_secs_for(retry_count: u32) -> u64 {
    (BASE_BACKOFF_SECS * 2u64.pow(retry_count)).min(MAX_BACKOFF_SECS)
}

fn completion_cache_capacity(table_count: usize) -> usize {
    table_count.clamp(MIN_COMPLETION_CACHE_CAPACITY, MAX_COMPLETION_CACHE_CAPACITY)
}

fn prefetch_table_detail(
    state: &mut AppState,
    run_id: u64,
    schema: &str,
    table: &str,
    now: Instant,
) -> Vec<Effect> {
    if !state.table_prefetch.is_current_prefetch_run(run_id) {
        return vec![];
    }
    let qualified_name = format!("{schema}.{table}");

    if state.table_prefetch.is_table_prefetching(&qualified_name) {
        return vec![];
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
            return effects;
        }

        let backoff_secs = backoff_secs_for(entry.retry_count);
        let elapsed = now.saturating_duration_since(entry.failed_at).as_secs();
        if elapsed < backoff_secs {
            let remaining = backoff_secs - elapsed;
            state.table_prefetch.queue_table_prefetch(qualified_name);
            return vec![Effect::DelayedProcessPrefetchQueue {
                run_id,
                delay_secs: remaining,
            }];
        }
    }

    let Some(dsn) = state.session.dsn().map(String::from) else {
        state.table_prefetch.defer_table_prefetch(qualified_name);
        return vec![];
    };

    state.table_prefetch.start_table_prefetch(qualified_name);

    vec![Effect::PrefetchTableColumnsAndFks {
        dsn,
        run_id,
        schema: schema.to_string(),
        table: table.to_string(),
    }]
}

pub(super) fn reduce_prefetch(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    if matches!(
        action,
        Action::StartErPrefetchAll
            | Action::StartErPrefetchScoped { .. }
            | Action::StartCompletionPrefetch { .. }
            | Action::ProcessPrefetchQueue { .. }
            | Action::PrefetchTableDetail { .. }
            | Action::TableDetailCached { .. }
            | Action::TableDetailCacheFailed { .. }
            | Action::TableDetailAlreadyCached { .. }
    ) && reject_pending_mysql_connection_probe(state)
    {
        return DispatchResult::handled();
    }

    match action {
        Action::StartErPrefetchAll => {
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

                let resize_capacity = completion_cache_capacity(metadata.table_summaries.len());

                for qualified_name in qualified_names {
                    state.table_prefetch.queue_table_prefetch(qualified_name);
                }
                DispatchResult::handled_with(vec![
                    Effect::ResizeCompletionCache {
                        capacity: resize_capacity,
                    },
                    Effect::SchedulePrefetchQueueProcessing { run_id },
                ])
            } else {
                DispatchResult::handled()
            }
        }

        Action::StartErPrefetchScoped { tables } => {
            if state.table_prefetch.active_prefetch_run_id().is_some()
                && state.table_prefetch.prefetch_tracks_er()
            {
                DispatchResult::handled()
            } else {
                let run_id = state.table_prefetch.begin_er_prefetch();
                state.er_preparation.begin_scoped_prefetch(tables);

                for qualified_name in tables {
                    state
                        .table_prefetch
                        .queue_table_prefetch(qualified_name.clone());
                }
                let mut effects = Vec::with_capacity(2);
                if let Some(metadata) = state.session.metadata() {
                    effects.push(Effect::ResizeCompletionCache {
                        capacity: completion_cache_capacity(metadata.table_summaries.len()),
                    });
                }
                effects.push(Effect::SchedulePrefetchQueueProcessing { run_id });
                DispatchResult::handled_with(effects)
            }
        }

        Action::StartCompletionPrefetch { tables } => {
            if tables.is_empty() {
                return DispatchResult::handled();
            }

            let run_id = if let Some(run_id) = state.table_prefetch.active_prefetch_run_id() {
                run_id
            } else {
                state.table_prefetch.begin_completion_prefetch()
            };
            for qualified_name in tables {
                state
                    .table_prefetch
                    .queue_table_prefetch(qualified_name.clone());
            }
            DispatchResult::handled_with(vec![Effect::SchedulePrefetchQueueProcessing { run_id }])
        }

        Action::ProcessPrefetchQueue { run_id } => {
            if !state.table_prefetch.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.table_prefetch.prefetch_in_flight_count();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            let queued_tables: Vec<String> = (0..available_slots)
                .filter_map(|_| state.table_prefetch.take_next_prefetch())
                .collect();
            let mut effects = Vec::new();
            for qualified_name in queued_tables {
                if let Some((schema, table)) = qualified_name.split_once('.') {
                    effects.extend(prefetch_table_detail(state, *run_id, schema, table, now));
                }
            }

            DispatchResult::handled_with(effects)
        }

        Action::PrefetchTableDetail {
            run_id,
            schema,
            table,
        } => {
            DispatchResult::handled_with(prefetch_table_detail(state, *run_id, schema, table, now))
        }

        Action::TableDetailCached {
            dsn,
            run_id,
            schema,
            table,
            detail,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.table_prefetch.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state
                .table_prefetch
                .complete_table_prefetch(&qualified_name);

            let mut effects = vec![Effect::CacheTableInCompletionEngine {
                qualified_name,
                table: detail.clone(),
            }];

            if state.table_prefetch.has_pending_prefetch() {
                effects.push(Effect::SchedulePrefetchQueueProcessing { run_id: *run_id });
            }

            if state.table_prefetch.prefetch_tracks_er() {
                effects.extend(check_er_completion(state));
            } else if state.modal.active_mode() == InputMode::SqlModal {
                effects.push(Effect::TriggerCompletion);
            }

            DispatchResult::handled_with(effects)
        }

        Action::TableDetailCacheFailed {
            dsn,
            run_id,
            schema,
            table,
            error,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.table_prefetch.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");

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
                effects.push(Effect::SchedulePrefetchQueueProcessing { run_id: *run_id });
            }
            effects.push(Effect::DelayedProcessPrefetchQueue {
                run_id: *run_id,
                delay_secs: backoff_secs_for(prev_count + 1),
            });

            if state.table_prefetch.prefetch_tracks_er() {
                effects.extend(check_er_completion(state));
            }

            DispatchResult::handled_with(effects)
        }

        Action::TableDetailAlreadyCached {
            dsn,
            run_id,
            schema,
            table,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.table_prefetch.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state
                .table_prefetch
                .complete_table_prefetch(&qualified_name);

            let mut effects = Vec::new();

            if state.table_prefetch.has_pending_prefetch() {
                effects.push(Effect::SchedulePrefetchQueueProcessing { run_id: *run_id });
            }

            if state.table_prefetch.prefetch_tracks_er() {
                effects.extend(check_er_completion(state));
            } else if state.modal.active_mode() == InputMode::SqlModal {
                effects.push(Effect::TriggerCompletion);
            }

            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}
