use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::TableSummary;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::FailedPrefetchEntry;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::check_er_completion;

const BASE_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 4;
const MIN_COMPLETION_CACHE_CAPACITY: usize = 500;
const MAX_COMPLETION_CACHE_CAPACITY: usize = 10_000;
pub(super) const MAX_PREFETCH_RETRIES: u32 = 3;

pub(super) fn backoff_secs_for(retry_count: u32) -> u64 {
    (BASE_BACKOFF_SECS * 2u64.pow(retry_count)).min(MAX_BACKOFF_SECS)
}

fn completion_cache_capacity(table_count: usize) -> usize {
    table_count.clamp(MIN_COMPLETION_CACHE_CAPACITY, MAX_COMPLETION_CACHE_CAPACITY)
}

pub(super) fn reduce_prefetch(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::StartPrefetchAll => {
            if (!state.sql_modal.is_prefetch_started() || !state.sql_modal.prefetch_tracks_er())
                && let Some(metadata) = state.session.metadata()
            {
                let run_id = state.sql_modal.begin_prefetch();
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
                    state.sql_modal.queue_table_prefetch(qualified_name);
                }
                DispatchResult::handled_with(vec![
                    Effect::ResizeCompletionCache {
                        capacity: resize_capacity,
                    },
                    Effect::ProcessPrefetchQueue { run_id },
                ])
            } else {
                DispatchResult::handled()
            }
        }

        Action::StartPrefetchScoped { tables } => {
            if state.sql_modal.is_prefetch_started() && state.sql_modal.prefetch_tracks_er() {
                DispatchResult::handled()
            } else {
                let run_id = state.sql_modal.begin_prefetch();
                state.er_preparation.begin_scoped_prefetch(tables);

                for qualified_name in tables {
                    state.sql_modal.queue_table_prefetch(qualified_name.clone());
                }
                let mut effects = Vec::with_capacity(2);
                if let Some(metadata) = state.session.metadata() {
                    effects.push(Effect::ResizeCompletionCache {
                        capacity: completion_cache_capacity(metadata.table_summaries.len()),
                    });
                }
                effects.push(Effect::ProcessPrefetchQueue { run_id });
                DispatchResult::handled_with(effects)
            }
        }

        Action::StartCompletionPrefetch { tables } => {
            if tables.is_empty() {
                return DispatchResult::handled();
            }

            let run_id = if let Some(run_id) = state.sql_modal.active_prefetch_run_id() {
                run_id
            } else {
                state.sql_modal.begin_completion_prefetch()
            };
            for qualified_name in tables {
                state.sql_modal.queue_table_prefetch(qualified_name.clone());
            }
            DispatchResult::handled_with(vec![Effect::ProcessPrefetchQueue { run_id }])
        }

        Action::ProcessPrefetchQueue { run_id } => {
            if !state.sql_modal.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetch_in_flight_count();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            let mut actions = Vec::new();
            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.take_next_prefetch()
                    && let Some((schema, table)) = qualified_name.split_once('.')
                {
                    actions.push(Action::PrefetchTableDetail {
                        run_id: *run_id,
                        schema: schema.to_string(),
                        table: table.to_string(),
                    });
                }
            }

            if actions.is_empty() {
                DispatchResult::handled()
            } else {
                DispatchResult::handled_with(vec![Effect::DispatchActions(actions)])
            }
        }

        Action::PrefetchTableDetail {
            run_id,
            schema,
            table,
        } => {
            if !state.sql_modal.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");

            if state.sql_modal.is_table_prefetching(&qualified_name) {
                return DispatchResult::handled();
            }

            if let Some(entry) = state.sql_modal.failed_prefetch(&qualified_name) {
                if entry.retry_count >= MAX_PREFETCH_RETRIES {
                    // Exceeded retry limit — give up, don't re-queue
                    let mut effects = if state.sql_modal.prefetch_tracks_er() {
                        state
                            .er_preparation
                            .on_table_failed(&qualified_name, entry.error.clone());
                        check_er_completion(state, now)
                    } else {
                        Vec::new()
                    };
                    // No fetch started → no completion event to re-drive the queue.
                    if effects.is_empty()
                        && state.sql_modal.prefetch_tracks_er()
                        && state.er_preparation.is_waiting()
                    {
                        effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
                    }
                    return DispatchResult::handled_with(effects);
                }

                let backoff_secs = backoff_secs_for(entry.retry_count);
                let elapsed = now.saturating_duration_since(entry.failed_at).as_secs();
                if elapsed < backoff_secs {
                    // Still in backoff — re-queue at tail and schedule a delayed retry
                    // to avoid busy-looping while waiting for the backoff to expire.
                    let remaining = backoff_secs - elapsed;
                    state.sql_modal.queue_table_prefetch(qualified_name);
                    return DispatchResult::handled_with(vec![
                        Effect::DelayedProcessPrefetchQueue {
                            run_id: *run_id,
                            delay_secs: remaining,
                        },
                    ]);
                }
            }

            let Some(dsn) = state.session.dsn().map(String::from) else {
                state.sql_modal.defer_table_prefetch(qualified_name);
                return DispatchResult::handled();
            };

            state.sql_modal.start_table_prefetch(qualified_name.clone());
            if state.sql_modal.prefetch_tracks_er() {
                state.er_preparation.start_fetching(&qualified_name);
            }

            DispatchResult::handled_with(vec![Effect::PrefetchTableDetail {
                dsn,
                run_id: *run_id,
                schema: schema.clone(),
                table: table.clone(),
            }])
        }

        Action::TableDetailCached {
            dsn,
            run_id,
            schema,
            table,
            detail,
        } => {
            if !state.session.dsn_matches(dsn) || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state.sql_modal.complete_table_prefetch(&qualified_name);
            if state.sql_modal.prefetch_tracks_er() {
                state.er_preparation.on_table_cached(&qualified_name);
            }

            let mut effects = vec![Effect::CacheTableInCompletionEngine {
                qualified_name,
                table: detail.clone(),
            }];

            if state.sql_modal.has_pending_prefetch() {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }

            if state.sql_modal.prefetch_tracks_er() {
                effects.extend(check_er_completion(state, now));
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
            if !state.session.dsn_matches(dsn) || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");

            let prev_count = state
                .sql_modal
                .failed_prefetch(&qualified_name)
                .map_or(0, |e| e.retry_count);
            let had_other_pending_before_requeue = state.sql_modal.retry_table_prefetch(
                qualified_name.clone(),
                FailedPrefetchEntry {
                    failed_at: now,
                    error: error.user_message(),
                    retry_count: prev_count + 1,
                },
            );
            if state.sql_modal.prefetch_tracks_er() {
                state.er_preparation.requeue_for_retry(&qualified_name);
            }

            let mut effects = Vec::new();

            if had_other_pending_before_requeue {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }
            effects.push(Effect::DelayedProcessPrefetchQueue {
                run_id: *run_id,
                delay_secs: backoff_secs_for(prev_count + 1),
            });

            if state.sql_modal.prefetch_tracks_er() {
                effects.extend(check_er_completion(state, now));
            }

            DispatchResult::handled_with(effects)
        }

        Action::TableDetailAlreadyCached {
            dsn,
            run_id,
            schema,
            table,
        } => {
            if !state.session.dsn_matches(dsn) || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state.sql_modal.complete_table_prefetch(&qualified_name);
            if state.sql_modal.prefetch_tracks_er() {
                state.er_preparation.on_table_cached(&qualified_name);
            }

            let mut effects = Vec::new();

            if state.sql_modal.has_pending_prefetch() {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }

            if state.sql_modal.prefetch_tracks_er() {
                effects.extend(check_er_completion(state, now));
            } else if state.modal.active_mode() == InputMode::SqlModal {
                effects.push(Effect::TriggerCompletion);
            }

            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}
