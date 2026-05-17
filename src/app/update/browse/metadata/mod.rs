use std::time::Instant;

mod er_neighbors;
mod loading;
mod prefetch;
mod table_detail;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn check_er_completion(state: &mut AppState) -> Vec<Effect> {
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

pub fn dispatch_metadata(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    loading::reduce_loading(state, action, now)
        .or_else(|| table_detail::reduce_table_detail(state, action))
        .or_else(|| prefetch::reduce_prefetch(state, action, now))
        .or_else(|| er_neighbors::reduce_er_neighbors(state, action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::effect::Effect;
    use crate::model::app_state::AppState;
    use crate::model::sql_editor::modal::FailedPrefetchEntry;
    use crate::update::action::Action;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.dsn = Some(dsn.to_string());
        state
    }

    fn empty_table(schema: &str, name: &str) -> Box<crate::domain::Table> {
        Box::new(crate::domain::Table {
            schema: schema.to_string(),
            name: name.to_string(),
            owner: None,
            columns: vec![],
            primary_key: None,
            indexes: vec![],
            foreign_keys: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: None,
            comment: None,
        })
    }

    mod freshness_guards {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn metadata_with_users() -> Arc<DatabaseMetadata> {
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )],
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn stale_metadata_loaded_does_not_replace_current_state() {
            let mut state = state_with_dsn("postgres://localhost/new");
            let run_id = state.session.begin_metadata_refresh();

            let effects = dispatch_metadata(
                &mut state,
                &Action::MetadataLoaded {
                    dsn: "postgres://localhost/old".to_string(),
                    run_id,
                    metadata: metadata_with_users(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.session.metadata().is_none());
        }

        #[test]
        fn stale_table_detail_loaded_does_not_replace_current_detail() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.session.begin_table_detail_run();
            let current_generation = state.session.selection_generation();
            let _ = state.session.begin_table_detail_run();

            dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    detail: empty_table("public", "users"),
                    generation: current_generation,
                },
                Instant::now(),
            );

            assert!(state.session.table_detail().is_none());
        }

        #[test]
        fn stale_prefetch_run_does_not_advance_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let old_run_id = state.sql_modal.begin_prefetch();
            let _ = state.sql_modal.begin_prefetch();
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id: old_run_id },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.sql_modal.prefetch_queue.len(), 1);
            assert!(state.sql_modal.prefetching_tables.is_empty());
        }
    }

    mod prefetch_table_detail {
        use super::prefetch::MAX_PREFETCH_RETRIES;
        use super::*;
        use crate::model::er_state::ErStatus;

        #[test]
        fn backoff_table_requeued_at_tail_with_process_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
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

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
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
        fn backoff_uses_injected_now() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            let failed_at = Instant::now();
            let now = failed_at.checked_add(Duration::from_secs(1)).unwrap();
            state.sql_modal.failed_prefetch_tables.insert(
                qualified,
                FailedPrefetchEntry {
                    failed_at,
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                now,
            )
            .unwrap();

            assert!(
                effects.iter().any(|e| matches!(
                    e,
                    Effect::DelayedProcessPrefetchQueue { delay_secs: 1, .. }
                ))
            );
        }

        #[test]
        fn no_dsn_requeues_without_marking_in_flight() {
            let mut state = AppState::new("test".to_string());
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            state
                .er_preparation
                .pending_tables
                .insert(qualified.clone());

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.sql_modal.prefetch_queue.front(), Some(&qualified));
            assert!(!state.sql_modal.prefetching_tables.contains(&qualified));
            assert!(!state.er_preparation.fetching_tables.contains(&qualified));
            assert!(state.er_preparation.pending_tables.contains(&qualified));
        }

        #[test]
        fn retry_limit_exceeded_gives_up_and_calls_on_table_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
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

            dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
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
            let run_id = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let qualified = "public.users".to_string();
            // Only table remaining; retry limit exceeded
            state
                .er_preparation
                .pending_tables
                .insert(qualified.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                qualified,
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
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
            let run_id = state.sql_modal.begin_prefetch();
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
            state.sql_modal.prefetch_queue.push_back(remaining);
            state.sql_modal.failed_prefetch_tables.insert(
                failed,
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue { .. }))
            );
            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
        }

        #[test]
        fn expired_backoff_proceeds_normally() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            // Failed 10 seconds ago with retry_count=1 (backoff = 2s, already expired)
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
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
        use crate::model::er_state::ErStatus;
        use crate::ports::outbound::DbOperationError;

        #[test]
        fn increments_retry_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now().checked_sub(Duration::from_secs(60)).unwrap(),
                    error: "old error".to_string(),
                    retry_count: 1,
                },
            );

            let now = Instant::now();
            dispatch_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::QueryFailed("new error".to_string()),
                },
                now,
            );

            let entry = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified)
                .unwrap();
            assert_eq!(entry.retry_count, 2);
            assert_eq!(
                entry.error,
                "Query failed: new error. Review the database error details and SQL."
            );
        }

        #[test]
        fn first_failure_sets_retry_count_1() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());

            let now = Instant::now();
            dispatch_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
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

        #[test]
        fn failure_requeues_table_for_retry_with_delayed_process() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());
            state
                .er_preparation
                .fetching_tables
                .insert(qualified.clone());

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.sql_modal.prefetch_queue.back(), Some(&qualified));
            assert!(state.er_preparation.pending_tables.contains(&qualified));
            assert!(!state.er_preparation.fetching_tables.contains(&qualified));
            assert!(!state.er_preparation.failed_tables.contains_key(&qualified));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DelayedProcessPrefetchQueue { .. }))
            );
            assert!(
                effects
                    .iter()
                    .all(|e| !matches!(e, Effect::ProcessPrefetchQueue { .. }))
            );
        }

        #[test]
        fn failure_continues_existing_queue_before_retry_delay() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            let failed = "public.users".to_string();
            let queued = "public.posts".to_string();
            state.sql_modal.prefetching_tables.insert(failed.clone());
            state.sql_modal.prefetch_queue.push_back(queued);
            state.er_preparation.fetching_tables.insert(failed);

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue { .. }))
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DelayedProcessPrefetchQueue { .. }))
            );
        }

        #[test]
        fn transient_failure_then_success_clears_er_failure_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let qualified = "public.users".to_string();
            state.sql_modal.prefetching_tables.insert(qualified.clone());
            state
                .er_preparation
                .fetching_tables
                .insert(qualified.clone());

            dispatch_metadata(
                &mut state,
                &Action::TableDetailCacheFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
            );
            state.sql_modal.prefetch_queue.clear();
            state.er_preparation.pending_tables.remove(&qualified);
            state
                .er_preparation
                .fetching_tables
                .insert(qualified.clone());

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailCached {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: empty_table("public", "users"),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.failed_tables.is_empty());
            assert!(
                effects
                    .iter()
                    .all(|effect| !matches!(effect, Effect::WriteErFailureLog { .. }))
            );
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|action| matches!(action, Action::ErGenerateFromCache))
            )));
        }
    }

    mod backoff_calculation {
        use super::prefetch::backoff_secs_for;

        #[test]
        fn backoff_values() {
            // retry_count 0 → 1s
            assert_eq!(backoff_secs_for(0), 1);
            // retry_count 1 → 2s
            assert_eq!(backoff_secs_for(1), 2);
            // retry_count 2 → 4s
            assert_eq!(backoff_secs_for(2), 4);
            // retry_count 3 → 4s (capped)
            assert_eq!(backoff_secs_for(3), 4);
        }
    }

    mod metadata_loaded {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(tables: Vec<(&str, &str)>) -> Arc<DatabaseMetadata> {
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None, false)
                    })
                    .collect(),
                fetched_at: Instant::now(),
            })
        }

        fn metadata_loaded_action(state: &mut AppState, metadata: Arc<DatabaseMetadata>) -> Action {
            let run_id = state.session.begin_metadata_refresh();
            Action::MetadataLoaded {
                dsn: "postgres://localhost/test".to_string(),
                run_id,
                metadata,
            }
        }

        #[test]
        fn table_disappeared_clears_pagination_and_result() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query.pagination);

            let metadata = make_metadata(vec![("public", "orders")]);
            let action = metadata_loaded_action(&mut state, metadata);
            dispatch_metadata(&mut state, &action, Instant::now());

            assert!(state.query.pagination.table.is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.table_detail().is_none());
            assert!(state.session.selected_table_key().is_none());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn table_still_exists_preserves_pagination_and_emits_refresh_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();

            // "orders" comes before "users" alphabetically, so "users" → index 1
            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = dispatch_metadata(&mut state, &action, Instant::now()).unwrap();

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
            let action = metadata_loaded_action(&mut state, metadata);
            dispatch_metadata(&mut state, &action, Instant::now());

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn after_connection_switch_pagination_reset_suppresses_auto_preview() {
            let mut state = state_with_dsn("postgres://localhost/test");
            // Simulate fresh connection: pagination is reset (as reset_connection_state does)
            state.query.pagination.reset();

            // New DB happens to have a table named "users" too
            let metadata = make_metadata(vec![("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = dispatch_metadata(&mut state, &action, Instant::now()).unwrap();

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
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: tables,
                fetched_at: Instant::now(),
            })
        }

        #[test]
        fn large_db_emits_resize_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(530)));

            let effects = dispatch_metadata(&mut state, &Action::StartPrefetchAll, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

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

            let effects = dispatch_metadata(&mut state, &Action::StartPrefetchAll, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

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

            dispatch_metadata(&mut state, &Action::StartPrefetchAll, Instant::now());

            assert!(state.er_preparation.fk_expanded);
        }
    }

    mod start_prefetch_scoped {
        use super::*;

        #[test]
        fn second_call_while_running_is_ignored() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.sql_modal.begin_prefetch();
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());

            let effects = dispatch_metadata(
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

            let effects = dispatch_metadata(
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
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue { .. }))
            );
        }
    }

    mod completion_check {
        use super::*;
        use crate::model::er_state::ErStatus;

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
        use super::prefetch::MAX_PREFETCH_RETRIES;
        use super::*;
        use crate::model::er_state::ErStatus;

        #[test]
        fn empty_neighbors_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;

            let effects = dispatch_metadata(
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
            let _ = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;

            let effects = dispatch_metadata(
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
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue { .. }))
            );
        }

        #[test]
        fn stale_neighbors_without_active_run_do_not_mutate_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.status = ErStatus::Waiting;

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            assert!(!state.er_preparation.fk_expanded);
            assert!(state.er_preparation.pending_tables.is_empty());
            assert!(state.sql_modal.prefetch_queue.is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn duplicate_neighbors_are_not_requeued() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state
                .er_preparation
                .pending_tables
                .insert("public.posts".to_string());
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.posts".to_string());
            state
                .sql_modal
                .prefetching_tables
                .insert("public.tags".to_string());

            dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    tables: vec![
                        "public.posts".to_string(),
                        "public.tags".to_string(),
                        "public.comments".to_string(),
                    ],
                },
                Instant::now(),
            );

            assert_eq!(state.sql_modal.prefetch_queue.len(), 2);
            assert_eq!(
                state
                    .sql_modal
                    .prefetch_queue
                    .iter()
                    .filter(|table| table.as_str() == "public.posts")
                    .count(),
                1
            );
            assert!(
                state
                    .sql_modal
                    .prefetch_queue
                    .contains(&"public.comments".to_string())
            );
            assert!(
                !state
                    .sql_modal
                    .prefetch_queue
                    .contains(&"public.tags".to_string())
            );
        }

        #[test]
        fn phase2_table_retry_limit_triggers_completion() {
            // All Phase 2 tables fail → completion must still fire
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_prefetch();
            state.er_preparation.status = ErStatus::Waiting;
            state.er_preparation.fk_expanded = true;
            let neighbor = "public.posts".to_string();
            state.er_preparation.pending_tables.insert(neighbor.clone());
            state.sql_modal.failed_prefetch_tables.insert(
                neighbor,
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
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
