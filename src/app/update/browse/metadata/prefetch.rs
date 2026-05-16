use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::model::sql_editor::modal::FailedPrefetchEntry;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

use super::{BASE_BACKOFF_SECS, MAX_BACKOFF_SECS, MAX_PREFETCH_RETRIES, check_er_completion};

pub(super) fn reduce_prefetch(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::StartPrefetchAll => {
            if !state.sql_modal.is_prefetch_started()
                && let Some(metadata) = state.session.metadata()
            {
                let run_id = state.sql_modal.begin_prefetch();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.total_tables = metadata.table_summaries.len();
                state.er_preparation.fk_expanded = true;

                let table_count = metadata.table_summaries.len();
                let resize_capacity = table_count.clamp(500, 10_000);

                for table_summary in &metadata.table_summaries {
                    let qualified_name = table_summary.qualified_name();
                    state
                        .sql_modal
                        .prefetch_queue
                        .push_back(qualified_name.clone());
                    state.er_preparation.pending_tables.insert(qualified_name);
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
            if state.sql_modal.is_prefetch_started() {
                DispatchResult::handled()
            } else {
                let run_id = state.sql_modal.begin_prefetch();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.fk_expanded = false;
                state.er_preparation.seed_tables.clone_from(tables);
                state.er_preparation.total_tables = tables.len();

                for qualified_name in tables {
                    state
                        .sql_modal
                        .prefetch_queue
                        .push_back(qualified_name.clone());
                    state
                        .er_preparation
                        .pending_tables
                        .insert(qualified_name.clone());
                }
                DispatchResult::handled_with(vec![Effect::ProcessPrefetchQueue { run_id }])
            }
        }

        Action::ProcessPrefetchQueue { run_id } => {
            if !state.sql_modal.is_current_prefetch_run(*run_id) {
                return DispatchResult::handled();
            }
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetching_tables.len();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            let mut actions = Vec::new();
            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.prefetch_queue.pop_front()
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

            if state.sql_modal.prefetching_tables.contains(&qualified_name) {
                return DispatchResult::handled();
            }

            if let Some(entry) = state.sql_modal.failed_prefetch_tables.get(&qualified_name) {
                if entry.retry_count >= MAX_PREFETCH_RETRIES {
                    // Exceeded retry limit — give up, don't re-queue
                    state.er_preparation.pending_tables.remove(&qualified_name);
                    state
                        .er_preparation
                        .on_table_failed(&qualified_name, entry.error.clone());
                    let mut effects = check_er_completion(state);
                    // No fetch started → no completion event to re-drive the queue.
                    if effects.is_empty() && state.er_preparation.status == ErStatus::Waiting {
                        effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
                    }
                    return DispatchResult::handled_with(effects);
                }

                let backoff_secs =
                    (BASE_BACKOFF_SECS * 2u64.pow(entry.retry_count)).min(MAX_BACKOFF_SECS);
                let elapsed = now.saturating_duration_since(entry.failed_at).as_secs();
                if elapsed < backoff_secs {
                    // Still in backoff — re-queue at tail and schedule a delayed retry
                    // to avoid busy-looping while waiting for the backoff to expire.
                    let remaining = backoff_secs - elapsed;
                    state.sql_modal.prefetch_queue.push_back(qualified_name);
                    return DispatchResult::handled_with(vec![
                        Effect::DelayedProcessPrefetchQueue {
                            run_id: *run_id,
                            delay_secs: remaining,
                        },
                    ]);
                }
            }

            let Some(dsn) = &state.session.dsn else {
                state.sql_modal.prefetch_queue.push_front(qualified_name);
                return DispatchResult::handled();
            };

            state
                .sql_modal
                .prefetching_tables
                .insert(qualified_name.clone());
            state.er_preparation.pending_tables.remove(&qualified_name);
            state
                .er_preparation
                .fetching_tables
                .insert(qualified_name.clone());

            DispatchResult::handled_with(vec![Effect::PrefetchTableDetail {
                dsn: dsn.clone(),
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
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = vec![Effect::CacheTableInCompletionEngine {
                qualified_name,
                table: detail.clone(),
            }];

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }

            effects.extend(check_er_completion(state));

            DispatchResult::handled_with(effects)
        }

        Action::TableDetailCacheFailed {
            dsn,
            run_id,
            schema,
            table,
            error,
        } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state.sql_modal.prefetching_tables.remove(&qualified_name);

            let prev_count = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified_name)
                .map_or(0, |e| e.retry_count);
            state.sql_modal.failed_prefetch_tables.insert(
                qualified_name.clone(),
                FailedPrefetchEntry {
                    failed_at: now,
                    error: error.user_message(),
                    retry_count: prev_count + 1,
                },
            );
            state.er_preparation.requeue_for_retry(&qualified_name);
            if !state.sql_modal.prefetch_queue.contains(&qualified_name) {
                state.sql_modal.prefetch_queue.push_back(qualified_name);
            }

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }

            effects.extend(check_er_completion(state));

            DispatchResult::handled_with(effects)
        }

        Action::TableDetailAlreadyCached {
            dsn,
            run_id,
            schema,
            table,
        } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.sql_modal.is_current_prefetch_run(*run_id)
            {
                return DispatchResult::handled();
            }
            let qualified_name = format!("{schema}.{table}");
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue { run_id: *run_id });
            }

            effects.extend(check_er_completion(state));

            DispatchResult::handled_with(effects)
        }
        _ => DispatchResult::pass(),
    }
}
