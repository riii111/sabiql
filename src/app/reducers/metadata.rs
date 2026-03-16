//! Metadata sub-reducer: metadata loading, table detail, and prefetch.

use std::sync::Arc;
use std::time::Instant;

use crate::app::action::{Action, TableTarget};
use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::effect::Effect;
use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::PREVIEW_PAGE_SIZE;
use crate::app::sql_modal_context::FailedPrefetchEntry;
use crate::app::state::AppState;
use crate::domain::MetadataState;

const BASE_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 4;
const MAX_PREFETCH_RETRIES: u32 = 3;

fn check_er_completion(state: &mut AppState) -> Vec<Effect> {
    if state.er_preparation.status != ErStatus::Waiting || !state.er_preparation.is_complete() {
        return vec![];
    }

    if !state.er_preparation.fk_expanded {
        return vec![Effect::DispatchActions(vec![
            Action::ExpandPrefetchWithFkNeighbors,
        ])];
    }

    if !state.er_preparation.has_failures() {
        state.er_preparation.status = ErStatus::Idle;
        return vec![Effect::DispatchActions(vec![Action::ErGenerateFromCache])];
    }

    state.er_preparation.status = ErStatus::Idle;
    let failed_data: Vec<(String, String)> = state
        .er_preparation
        .failed_tables
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    state.set_error(format!(
        "ER failed: {} table(s) failed. 'e' to retry.",
        failed_data.len()
    ));
    vec![Effect::WriteErFailureLog {
        failed_tables: failed_data,
    }]
}

/// Handles metadata loading, table detail, and prefetch actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_metadata(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::MetadataLoaded(metadata) => {
            let has_tables = !metadata.tables.is_empty();
            state.session.mark_connected(Arc::clone(metadata));

            let mut effects = vec![];

            if !state.query.pagination.table.is_empty() {
                let prev_schema = &state.query.pagination.schema;
                let prev_table = &state.query.pagination.table;
                let found_index = metadata
                    .tables
                    .iter()
                    .position(|t| &t.schema == prev_schema && &t.name == prev_table);
                match found_index {
                    Some(idx) => {
                        state.ui.set_explorer_selection(Some(idx));
                        // Refresh preview and detail: DDL or reload may have changed
                        // data/schema even though the table still exists.
                        if let Some(dsn) = &state.session.dsn {
                            let page = state.query.pagination.current_page;
                            let generation = state.session.selection_generation();
                            effects.push(Effect::ExecutePreview {
                                dsn: dsn.clone(),
                                schema: state.query.pagination.schema.clone(),
                                table: state.query.pagination.table.clone(),
                                generation,
                                limit: PREVIEW_PAGE_SIZE,
                                offset: page * PREVIEW_PAGE_SIZE,
                                target_page: page,
                                read_only: state.session.read_only,
                            });
                            effects.push(Effect::FetchTableDetail {
                                dsn: dsn.clone(),
                                schema: state.query.pagination.schema.clone(),
                                table: state.query.pagination.table.clone(),
                                generation,
                            });
                        }
                    }
                    None => {
                        // The previously selected table was removed (e.g. via DROP TABLE).
                        // Clear all selection state to avoid stale references.
                        state
                            .ui
                            .set_explorer_selection(if has_tables { Some(0) } else { None });
                        state
                            .session
                            .clear_table_selection(&mut state.query.pagination);
                        state.query.clear_current_result();
                    }
                }
            } else {
                state
                    .ui
                    .set_explorer_selection(if has_tables { Some(0) } else { None });
            }

            state.connection_error.clear();

            if state.session.is_reloading {
                state.messages.set_success_at("Reloaded!".to_string(), now);
                state.session.is_reloading = false;
            }

            if state.modal.active_mode() == InputMode::SqlModal
                && !state.sql_modal.is_prefetch_started()
            {
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchAll]));
            }

            if state.ui.pending_er_picker && state.modal.active_mode() == InputMode::Normal {
                state.ui.pending_er_picker = false;
                effects.push(Effect::DispatchActions(vec![Action::OpenErTablePicker]));
            } else {
                state.ui.pending_er_picker = false;
            }

            Some(effects)
        }
        Action::MetadataFailed(error) => {
            let error_info = ConnectionErrorInfo::new(error);
            state.connection_error.set_error(error_info);
            let was_connected = state.session.connection_state().is_connected();
            state.session.mark_connection_failed(error.clone());
            if !was_connected {
                state.modal.replace_mode(InputMode::ConnectionError);
            }
            if state.er_preparation.status == ErStatus::Waiting {
                state.er_preparation.status = ErStatus::Idle;
            }
            Some(vec![])
        }
        Action::TableDetailLoaded(detail, generation) => {
            if state.session.set_table_detail(*detail.clone(), *generation) {
                state.ui.inspector_scroll_offset = 0;
            }
            Some(vec![])
        }
        Action::TableDetailFailed(error, generation) => {
            if *generation == state.session.selection_generation() {
                state.set_error(error.clone());
            }
            Some(vec![])
        }

        Action::LoadMetadata => {
            if let Some(dsn) = state.session.dsn.clone() {
                state.session.set_metadata_state(MetadataState::Loading);
                Some(vec![Effect::FetchMetadata { dsn }])
            } else {
                Some(vec![])
            }
        }
        Action::LoadTableDetail(TableTarget {
            schema,
            table,
            generation,
        }) => {
            if let Some(dsn) = &state.session.dsn {
                Some(vec![Effect::FetchTableDetail {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ReloadMetadata => {
            if let Some(dsn) = state.session.dsn.clone() {
                state.session.begin_reload();
                state.sql_modal.reset_prefetch();
                state.er_preparation.reset();
                state.ui.er_selected_tables.clear();
                state.ui.pending_er_picker = false;
                state.messages.last_error = None;
                state.messages.last_success = None;
                state.messages.expires_at = None;

                Some(vec![Effect::Sequence(vec![
                    Effect::CacheInvalidate { dsn: dsn.clone() },
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata { dsn },
                ])])
            } else {
                Some(vec![])
            }
        }

        Action::StartPrefetchAll => {
            if !state.sql_modal.is_prefetch_started()
                && let Some(metadata) = state.session.metadata()
            {
                state.sql_modal.begin_prefetch();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.total_tables = metadata.tables.len();
                state.er_preparation.fk_expanded = true;

                let table_count = metadata.tables.len();
                let resize_capacity = table_count.clamp(500, 10_000);

                for table_summary in &metadata.tables {
                    let qualified_name = table_summary.qualified_name();
                    state
                        .sql_modal
                        .prefetch_queue
                        .push_back(qualified_name.clone());
                    state.er_preparation.pending_tables.insert(qualified_name);
                }
                Some(vec![
                    Effect::ResizeCompletionCache {
                        capacity: resize_capacity,
                    },
                    Effect::ProcessPrefetchQueue,
                ])
            } else {
                Some(vec![])
            }
        }

        Action::StartPrefetchScoped { tables } => {
            if !state.sql_modal.is_prefetch_started() {
                state.sql_modal.begin_prefetch();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.fk_expanded = false;
                state.er_preparation.seed_tables = tables.clone();
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
                Some(vec![Effect::ProcessPrefetchQueue])
            } else {
                Some(vec![])
            }
        }

        Action::ExpandPrefetchWithFkNeighbors => {
            let seed_tables = state.er_preparation.seed_tables.clone();
            Some(vec![Effect::ExtractFkNeighbors { seed_tables }])
        }

        Action::FkNeighborsDiscovered { tables } => {
            state.er_preparation.fk_expanded = true;

            if tables.is_empty() {
                // No new neighbors — proceed to generate with what we have
                return Some(check_er_completion(state));
            }

            for qualified_name in tables {
                state
                    .er_preparation
                    .pending_tables
                    .insert(qualified_name.clone());
                state
                    .sql_modal
                    .prefetch_queue
                    .push_back(qualified_name.clone());
            }
            Some(vec![Effect::ProcessPrefetchQueue])
        }

        Action::ProcessPrefetchQueue => {
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetching_tables.len();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            let mut actions = Vec::new();
            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.prefetch_queue.pop_front()
                    && let Some((schema, table)) = qualified_name.split_once('.')
                {
                    actions.push(Action::PrefetchTableDetail {
                        schema: schema.to_string(),
                        table: table.to_string(),
                    });
                }
            }

            if actions.is_empty() {
                Some(vec![])
            } else {
                Some(vec![Effect::DispatchActions(actions)])
            }
        }

        Action::PrefetchTableDetail { schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);

            if state.sql_modal.prefetching_tables.contains(&qualified_name) {
                return Some(vec![]);
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
                        effects.push(Effect::ProcessPrefetchQueue);
                    }
                    return Some(effects);
                }

                let backoff_secs =
                    (BASE_BACKOFF_SECS * 2u64.pow(entry.retry_count)).min(MAX_BACKOFF_SECS);
                let elapsed = entry.failed_at.elapsed().as_secs();
                if elapsed < backoff_secs {
                    // Still in backoff — re-queue at tail and schedule a delayed retry
                    // to avoid busy-looping while waiting for the backoff to expire.
                    let remaining = backoff_secs - elapsed;
                    state.sql_modal.prefetch_queue.push_back(qualified_name);
                    return Some(vec![Effect::DelayedProcessPrefetchQueue {
                        delay_secs: remaining,
                    }]);
                }
            }

            state
                .sql_modal
                .prefetching_tables
                .insert(qualified_name.clone());
            state.er_preparation.pending_tables.remove(&qualified_name);
            state
                .er_preparation
                .fetching_tables
                .insert(qualified_name.clone());

            if let Some(dsn) = &state.session.dsn {
                Some(vec![Effect::PrefetchTableDetail {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                }])
            } else {
                Some(vec![])
            }
        }

        Action::TableDetailCached {
            schema,
            table,
            detail,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
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
                effects.push(Effect::ProcessPrefetchQueue);
            }

            effects.extend(check_er_completion(state));

            Some(effects)
        }

        Action::TableDetailCacheFailed {
            schema,
            table,
            error,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);

            let prev_count = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified_name)
                .map(|e| e.retry_count)
                .unwrap_or(0);
            state.sql_modal.failed_prefetch_tables.insert(
                qualified_name.clone(),
                FailedPrefetchEntry {
                    failed_at: now,
                    error: error.clone(),
                    retry_count: prev_count + 1,
                },
            );
            state
                .er_preparation
                .on_table_failed(&qualified_name, error.clone());

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            effects.extend(check_er_completion(state));

            Some(effects)
        }

        Action::TableDetailAlreadyCached { schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            effects.extend(check_er_completion(state));

            Some(effects)
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::sql_modal_context::FailedPrefetchEntry;
    use crate::app::state::AppState;
    use std::time::{Duration, Instant};

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.dsn = Some(dsn.to_string());
        state
    }

    mod prefetch_table_detail {
        use super::*;
        use crate::app::er_state::ErStatus;

        #[test]
        fn backoff_table_requeued_at_tail_with_process_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            // Insert a recently failed entry (retry_count=1, just failed)
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            // Should be re-queued at tail
            assert_eq!(state.sql_modal.prefetch_queue.back(), Some(&qualified));
            // Should return DelayedProcessPrefetchQueue (not an immediate busy-loop)
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DelayedProcessPrefetchQueue { .. }))
            );
        }

        #[test]
        fn retry_limit_exceeded_gives_up_and_calls_on_table_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            state
                .er_preparation
                .pending_tables
                .insert(qualified.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            );

            assert!(!state.sql_modal.prefetch_queue.contains(&qualified));
            assert!(state.er_preparation.failed_tables.contains_key(&qualified));
            assert!(!state.er_preparation.pending_tables.contains(&qualified));
        }

        #[test]
        fn retry_limit_exceeded_as_last_table_triggers_er_completion() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let qualified = "public.users".to_string();
            // Only table remaining; retry limit exceeded
            state
                .er_preparation
                .pending_tables
                .insert(qualified.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::WriteErFailureLog { .. }))
            );
        }

        #[test]
        fn retry_limit_exceeded_with_queue_remaining_redrives_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let failed = "public.users".to_string();
            let remaining = "public.posts".to_string();
            // users exhausted retries; posts still awaiting in queue
            state.er_preparation.pending_tables.insert(failed.clone());
            state
                .er_preparation
                .pending_tables
                .insert(remaining.clone());
            state.sql_modal.prefetch_queue.push_back(remaining.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                failed.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue))
            );
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
        }

        #[test]
        fn expired_backoff_proceeds_normally() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            // Failed 10 seconds ago with retry_count=1 (backoff = 2s, already expired)
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now() - Duration::from_secs(10),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            // Should proceed to fetching
            assert!(state.sql_modal.prefetching_tables.contains(&qualified));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::PrefetchTableDetail { .. }))
            );
        }
    }

    mod table_detail_cache_failed {
        use super::*;

        #[test]
        fn increments_retry_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now() - Duration::from_secs(60),
                    error: "old error".to_string(),
                    retry_count: 1,
                },
            );

            let now = Instant::now();
            reduce_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: "new error".to_string(),
                },
                now,
            );

            let entry = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified)
                .unwrap();
            assert_eq!(entry.retry_count, 2);
            assert_eq!(entry.error, "new error");
        }

        #[test]
        fn first_failure_sets_retry_count_1() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());

            let now = Instant::now();
            reduce_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: "timeout".to_string(),
                },
                now,
            );

            let entry = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified)
                .unwrap();
            assert_eq!(entry.retry_count, 1);
        }
    }

    mod backoff_calculation {
        use super::*;

        #[test]
        fn backoff_values() {
            // retry_count 0 → 1s
            assert_eq!((BASE_BACKOFF_SECS * 2u64.pow(0)).min(MAX_BACKOFF_SECS), 1);
            // retry_count 1 → 2s
            assert_eq!((BASE_BACKOFF_SECS * 2u64.pow(1)).min(MAX_BACKOFF_SECS), 2);
            // retry_count 2 → 4s
            assert_eq!((BASE_BACKOFF_SECS * 2u64.pow(2)).min(MAX_BACKOFF_SECS), 4);
            // retry_count 3 → 4s (capped)
            assert_eq!((BASE_BACKOFF_SECS * 2u64.pow(3)).min(MAX_BACKOFF_SECS), 4);
        }
    }

    mod metadata_loaded {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(tables: Vec<(&str, &str)>) -> Arc<DatabaseMetadata> {
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None, false)
                    })
                    .collect(),
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn table_disappeared_clears_pagination_and_result() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query.pagination);

            let metadata = make_metadata(vec![("public", "orders")]);
            reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            );

            assert!(state.query.pagination.table.is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.table_detail().is_none());
            assert!(state.session.current_table().is_none());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn table_still_exists_preserves_pagination_and_emits_refresh_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();

            // "orders" comes before "users" alphabetically, so "users" → index 1
            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let effects = reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.query.pagination.table, "users");
            assert_eq!(state.ui.explorer_selected, 1);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { table, .. } if table == "users"))
            );
            assert!(
                effects.iter().any(
                    |e| matches!(e, Effect::FetchTableDetail { table, .. } if table == "users")
                )
            );
        }

        #[test]
        fn no_table_selected_defaults_to_first() {
            let mut state = state_with_dsn("postgres://localhost/test");

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn after_connection_switch_pagination_reset_suppresses_auto_preview() {
            let mut state = state_with_dsn("postgres://localhost/test");
            // Simulate fresh connection: pagination is reset (as reset_connection_state does)
            state.query.pagination.reset();

            // New DB happens to have a table named "users" too
            let metadata = make_metadata(vec![("public", "users")]);
            let effects = reduce_metadata(
                &mut state,
                &Action::MetadataLoaded(metadata),
                Instant::now(),
            )
            .unwrap();

            // No table was selected on this connection, so no auto-preview should fire
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::ExecutePreview { .. }))
            );
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::FetchTableDetail { .. }))
            );
        }
    }

    mod start_prefetch_all {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{}", i), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn large_db_emits_resize_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(530)));

            let effects =
                reduce_metadata(&mut state, &Action::StartPrefetchAll, Instant::now()).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ResizeCompletionCache { capacity: 530 }))
            );
        }

        #[test]
        fn small_db_uses_floor_capacity() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(50)));

            let effects =
                reduce_metadata(&mut state, &Action::StartPrefetchAll, Instant::now()).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ResizeCompletionCache { capacity: 500 }))
            );
        }

        #[test]
        fn sets_fk_expanded_true() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(10)));

            reduce_metadata(&mut state, &Action::StartPrefetchAll, Instant::now());

            assert!(state.er_preparation.fk_expanded);
        }
    }

    mod start_prefetch_scoped {
        use super::*;

        #[test]
        fn second_call_while_running_is_ignored() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());

            let effects = reduce_metadata(
                &mut state,
                &Action::StartPrefetchScoped {
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            // In-progress prefetch must not be silently reset
            assert!(state.er_preparation.pending_tables.contains("public.users"));
            assert!(effects.is_empty());
        }

        #[test]
        fn only_selected_tables_in_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            let effects = reduce_metadata(
                &mut state,
                &Action::StartPrefetchScoped {
                    tables: tables.clone(),
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.sql_modal.prefetch_queue.len(), 2);
            assert!(state.er_preparation.pending_tables.contains("public.users"));
            assert!(
                state
                    .er_preparation
                    .pending_tables
                    .contains("public.orders")
            );
            assert!(!state.er_preparation.fk_expanded);
            assert_eq!(state.er_preparation.seed_tables, tables);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue))
            );
        }
    }

    mod completion_check {
        use super::*;
        use crate::app::er_state::ErStatus;

        #[test]
        fn complete_not_fk_expanded_dispatches_expand() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = false;
            // pending and fetching are empty → is_complete() = true

            let effects = check_er_completion(&mut state);

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ExpandPrefetchWithFkNeighbors))
            )));
        }

        #[test]
        fn complete_fk_expanded_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;

            let effects = check_er_completion(&mut state);

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }
    }

    mod fk_neighbors_discovered {
        use super::*;
        use crate::app::er_state::ErStatus;

        #[test]
        fn empty_neighbors_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;

            let effects = reduce_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered { tables: vec![] },
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.fk_expanded);
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn non_empty_neighbors_adds_to_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;

            let effects = reduce_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    tables: vec!["public.posts".to_string(), "public.tags".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.fk_expanded);
            assert!(state.er_preparation.pending_tables.contains("public.posts"));
            assert!(state.er_preparation.pending_tables.contains("public.tags"));
            assert_eq!(state.sql_modal.prefetch_queue.len(), 2);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue))
            );
        }

        #[test]
        fn phase2_table_retry_limit_triggers_completion() {
            // All Phase 2 tables fail → completion must still fire
            let mut state = state_with_dsn("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let neighbor = "public.posts".to_string();
            state.er_preparation.pending_tables.insert(neighbor.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                neighbor.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = reduce_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    schema: "public".to_string(),
                    table: "posts".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::WriteErFailureLog { .. }))
            );
        }
    }
}
