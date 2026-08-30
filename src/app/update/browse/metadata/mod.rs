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
    if state.er_preparation.status() != ErStatus::Waiting || !state.er_preparation.is_complete() {
        return vec![];
    }

    if !state.er_preparation.fk_expanded() {
        let Some(run_id) = state.sql_modal.active_prefetch_run_id() else {
            return vec![];
        };
        return er_neighbors::expand_prefetch_with_fk_neighbors(state, run_id);
    }

    if !state.er_preparation.has_failures() {
        state.er_preparation.mark_idle();
        return vec![Effect::DispatchActions(vec![Action::ErGenerateFromCache])];
    }

    state.er_preparation.mark_idle();
    let failed_data: Vec<(String, String)> = state.er_preparation.failed_table_errors();
    state.messages.set_error(format!(
        "ER failed: {} table(s) failed. 'e' to retry.",
        failed_data.len()
    ));
    vec![Effect::WriteErFailureLog {
        failed_tables: failed_data,
    }]
}

pub fn dispatch_metadata(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    loading::reduce_loading(state, action, now)
        .or_else(|| table_detail::reduce_table_detail(state, action, now))
        .or_else(|| prefetch::reduce_prefetch(state, action, now))
        .or_else(|| er_neighbors::reduce_er_neighbors(state, action, now))
}

#[cfg(test)]
mod tests {
    use crate::test_support;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::domain::{ConnectionId, DatabaseType, Table};
    use crate::model::app_state::AppState;
    use crate::model::browse::session::TableDetailState;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::sql_editor::modal::FailedPrefetchEntry;
    use crate::ports::outbound::DbOperationError;
    use crate::update::action::Action;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "postgres",
            DatabaseType::PostgreSQL,
            dsn,
        );
        state
    }

    fn sqlite_state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "sqlite",
            DatabaseType::SQLite,
            dsn,
        );
        state
    }

    fn state_with_pending_mysql_probe() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_target(
            &ConnectionId::from_string("mysql-current"),
            "mysql-current",
            DatabaseType::MySQL,
            "mysql://localhost/current",
            Some("current"),
        );
        let _ = state.session.begin_mysql_connection_probe(
            &ConnectionId::from_string("mysql-target"),
            "mysql-target",
            "mysql://localhost/target",
            Some("target"),
        );
        state
    }

    fn empty_table(schema: &str, name: &str) -> Box<Table> {
        Box::new(test_support::table::minimal(schema, name))
    }

    mod freshness_guards {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};
        use crate::model::connection::state::ConnectionState;

        fn metadata_with_users() -> Arc<DatabaseMetadata> {
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )];
                metadata
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
        fn current_table_detail_failure_updates_inspector_and_footer() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let run_id = state.session.begin_table_detail_run();

            dispatch_metadata(
                &mut state,
                &Action::TableDetailFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id,
                    error: DbOperationError::PermissionDenied("denied".to_string()),
                    generation,
                },
                Instant::now(),
            );

            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
            assert!(state.messages.last_error().is_some());
        }

        #[test]
        fn metadata_failure_rejects_late_detail_completion() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state
                .session
                .mark_connected(Arc::new(DatabaseMetadata::new("test".to_string())));
            let generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let detail_run_id = state.session.begin_table_detail_run();
            let metadata_run_id = state.session.begin_metadata_refresh();

            dispatch_metadata(
                &mut state,
                &Action::MetadataFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: metadata_run_id,
                    error: DbOperationError::PermissionDenied("denied".to_string()),
                },
                Instant::now(),
            );
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
            assert!(!state.session.is_current_table_detail_run(detail_run_id));

            dispatch_metadata(
                &mut state,
                &Action::TableDetailLoaded {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: detail_run_id,
                    detail: empty_table("public", "users"),
                    generation,
                },
                Instant::now(),
            );

            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
        }

        #[test]
        fn stale_table_detail_failure_does_not_update_current_inspector() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let stale_generation = state
                .session
                .select_table("public", "users", &mut state.query);
            let _ = state.session.begin_table_detail_run();
            let current_generation =
                state
                    .session
                    .select_table("public", "orders", &mut state.query);
            let current_run_id = state.session.begin_table_detail_run();

            dispatch_metadata(
                &mut state,
                &Action::TableDetailFailed {
                    dsn: "postgres://localhost/test".to_string(),
                    run_id: current_run_id,
                    error: DbOperationError::PermissionDenied("denied".to_string()),
                    generation: stale_generation,
                },
                Instant::now(),
            );

            assert_eq!(state.session.selection_generation(), current_generation);
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Loading
            ));
            assert!(state.messages.last_error().is_none());
        }

        #[test]
        fn stale_prefetch_run_does_not_advance_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let old_run_id = state.sql_modal.begin_er_prefetch();
            let _ = state.sql_modal.begin_er_prefetch();
            state
                .sql_modal
                .queue_table_prefetch("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id: old_run_id },
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.sql_modal.has_pending_prefetch());
            assert!(state.sql_modal.is_prefetch_queued("public.users"));
            assert_eq!(state.sql_modal.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn mysql_reload_metadata_starts_fetch() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_target(
                &ConnectionId::new(),
                "mysql",
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
                Some("app"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            let effects = dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now())
                .into_effects()
                .unwrap();

            assert!(effects.iter().any(contains_fetch_metadata));
            assert!(state.messages.last_error().is_none());
        }

        #[test]
        fn mysql_reload_metadata_during_pending_switch_preserves_probe() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::from_string("mysql-a");
            let target_id = ConnectionId::from_string("mysql-b");
            state.session.activate_connection_with_target(
                &current_id,
                "mysql-a",
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/a",
                Some("a"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let probe_run_id = state.session.begin_mysql_connection_probe(
                &target_id,
                "mysql-b",
                "mysql://user@localhost:3306/b",
                Some("b"),
            );

            let effects = dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now())
                .into_effects()
                .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(probe_run_id)
            );
            assert_eq!(
                state.messages.last_error(),
                Some("Connection switch in progress")
            );
        }

        #[test]
        fn mysql_reload_metadata_during_same_connection_retry_preserves_probe() {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::from_string("mysql-a");
            let dsn = "mysql://user@localhost:3306/a";
            state.session.activate_connection_with_target(
                &id,
                "mysql-a",
                DatabaseType::MySQL,
                dsn,
                Some("a"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let probe_run_id =
                state
                    .session
                    .begin_mysql_connection_probe(&id, "mysql-a", dsn, Some("a"));

            let effects = dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now())
                .into_effects()
                .unwrap();

            assert!(effects.is_empty());
            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| pending.run_id),
                Some(probe_run_id)
            );
            assert_eq!(
                state.messages.last_error(),
                Some("Connection switch in progress")
            );
        }

        fn contains_fetch_metadata(effect: &Effect) -> bool {
            match effect {
                Effect::FetchMetadata { .. } => true,
                Effect::Sequence(effects) => effects.iter().any(contains_fetch_metadata),
                _ => false,
            }
        }
    }

    mod prefetch_table_detail {
        use super::prefetch::MAX_PREFETCH_RETRIES;
        use super::*;

        #[test]
        fn backoff_table_requeued_at_tail_with_process_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            let queued = "public.orders".to_string();
            state.sql_modal.queue_table_prefetch(queued.clone());
            // Insert a recently failed entry (retry_count=1, just failed)
            state.sql_modal.fail_table_prefetch(
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
            assert_eq!(state.sql_modal.take_next_prefetch(), Some(queued));
            assert_eq!(state.sql_modal.take_next_prefetch(), Some(qualified));
            assert!(!state.sql_modal.has_pending_prefetch());
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
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            let failed_at = Instant::now();
            let now = failed_at.checked_add(Duration::from_secs(1)).unwrap();
            state.sql_modal.fail_table_prefetch(
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
        fn process_queue_does_not_reprocess_requeued_backoff_table() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );
            state.sql_modal.queue_table_prefetch(qualified.clone());

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(
                effects
                    .iter()
                    .filter(|effect| matches!(effect, Effect::DelayedProcessPrefetchQueue { .. }))
                    .count(),
                1
            );
            assert_eq!(state.sql_modal.take_next_prefetch(), Some(qualified));
            assert!(!state.sql_modal.has_pending_prefetch());
        }

        #[test]
        fn no_dsn_requeues_without_marking_in_flight() {
            let mut state = AppState::new("test".to_string());
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.er_preparation.queue_pending_table(qualified.clone());

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
            assert!(state.sql_modal.is_prefetch_queued(&qualified));
            assert!(!state.sql_modal.is_table_prefetching(&qualified));
            assert!(!state.er_preparation.fetching_tables().contains(&qualified));
            assert!(state.er_preparation.pending_tables().contains(&qualified));
        }

        #[test]
        fn pending_mysql_probe_rejects_direct_prefetch_without_starting_it() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.sql_modal.begin_completion_prefetch();

            let effects = dispatch_metadata(
                &mut state,
                &Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
            )
            .into_effects()
            .expect("prefetch action should be handled");

            assert!(effects.is_empty());
            assert!(!state.sql_modal.is_table_prefetching("public.users"));
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_completion_without_cache_mutation() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.sql_modal.begin_completion_prefetch();
            state
                .sql_modal
                .start_table_prefetch("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::TableDetailCached {
                    dsn: "mysql://localhost/current".to_string(),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: empty_table("public", "users"),
                },
                Instant::now(),
            )
            .into_effects()
            .expect("cached detail action should be handled");

            assert!(effects.is_empty());
            assert!(state.sql_modal.is_table_prefetching("public.users"));
        }

        #[test]
        fn retry_limit_exceeded_gives_up_and_calls_on_table_failed() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.er_preparation.queue_pending_table(qualified.clone());
            state.sql_modal.fail_table_prefetch(
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

            assert!(!state.sql_modal.is_prefetch_queued(&qualified));
            assert!(
                state
                    .er_preparation
                    .failed_tables()
                    .contains_key(&qualified)
            );
            assert!(!state.er_preparation.pending_tables().contains(&qualified));
        }

        #[test]
        fn retry_limit_exceeded_as_last_table_triggers_er_completion() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_expanded();
            let qualified = "public.users".to_string();
            // Only table remaining; retry limit exceeded
            state.er_preparation.queue_pending_table(qualified.clone());
            state.sql_modal.fail_table_prefetch(
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

            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::WriteErFailureLog { .. }))
            );
        }

        #[test]
        fn retry_limit_exceeded_with_queue_remaining_redrives_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_expanded();
            let failed = "public.users".to_string();
            let remaining = "public.posts".to_string();
            // users exhausted retries; posts still awaiting in queue
            state.er_preparation.queue_pending_table(failed.clone());
            state.er_preparation.queue_pending_table(remaining.clone());
            state.sql_modal.queue_table_prefetch(remaining);
            state.sql_modal.fail_table_prefetch(
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
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
        }

        #[test]
        fn expired_backoff_proceeds_normally() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            // Failed 10 seconds ago with retry_count=1 (backoff = 2s, already expired)
            state.sql_modal.fail_table_prefetch(
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
            assert!(state.sql_modal.is_table_prefetching(&qualified));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::PrefetchTableColumnsAndFks { .. }))
            );
        }
    }

    mod table_detail_cache_failed {
        use super::*;

        #[test]
        fn increments_retry_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now().checked_sub(Duration::from_mins(1)).unwrap(),
                    error: "old error".to_string(),
                    retry_count: 1,
                },
            );
            state.sql_modal.start_table_prefetch(qualified.clone());

            assert!(state.sql_modal.is_table_prefetching(&qualified));

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

            let entry = state.sql_modal.failed_prefetch(&qualified).unwrap();
            assert_eq!(entry.retry_count, 2);
            assert!(!state.sql_modal.is_table_prefetching(&qualified));
            assert_eq!(
                entry.error,
                "Query failed: new error. Review the database error details and SQL."
            );
        }

        #[test]
        fn first_failure_sets_retry_count_1() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.start_table_prefetch(qualified.clone());

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

            let entry = state.sql_modal.failed_prefetch(&qualified).unwrap();
            assert_eq!(entry.retry_count, 1);
        }

        #[test]
        fn failure_requeues_table_for_retry_with_delayed_process() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.sql_modal.start_table_prefetch(qualified.clone());
            state.er_preparation.start_fetching(&qualified);

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

            assert!(state.sql_modal.is_prefetch_queued(&qualified));
            assert!(state.er_preparation.pending_tables().contains(&qualified));
            assert!(!state.er_preparation.fetching_tables().contains(&qualified));
            assert!(
                !state
                    .er_preparation
                    .failed_tables()
                    .contains_key(&qualified)
            );
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DelayedProcessPrefetchQueue { .. }))
            );
            assert!(
                effects
                    .iter()
                    .all(|e| !matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
        }

        #[test]
        fn failure_continues_existing_queue_before_retry_delay() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            let failed = "public.users".to_string();
            let queued = "public.posts".to_string();
            state.sql_modal.start_table_prefetch(failed.clone());
            state.sql_modal.queue_table_prefetch(queued);
            state.er_preparation.start_fetching(&failed);

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
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
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
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_expanded();
            let qualified = "public.users".to_string();
            state.sql_modal.start_table_prefetch(qualified.clone());
            state.er_preparation.start_fetching(&qualified);

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
            let _ = state.sql_modal.take_next_prefetch();
            state
                .er_preparation
                .on_table_failed(&qualified, "timed out".to_string());
            assert!(!state.er_preparation.failed_tables().is_empty());
            state.er_preparation.start_fetching(&qualified);

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

            assert!(state.er_preparation.failed_tables().is_empty());
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
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables
                    .into_iter()
                    .map(|(schema, name)| {
                        TableSummary::new(schema.to_string(), name.to_string(), None, false)
                    })
                    .collect();
                metadata
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
                .select_table("public", "users", &mut state.query);

            let metadata = make_metadata(vec![("public", "orders")]);
            let action = metadata_loaded_action(&mut state, metadata);
            dispatch_metadata(&mut state, &action, Instant::now());

            assert!(state.query.pagination.table().is_empty());
            assert!(state.query.current_result().is_none());
            assert_eq!(state.query.result_generation(), 2);
            assert!(state.session.table_detail().is_none());
            assert!(state.session.selected_table_key().is_none());
            assert_eq!(state.ui.explorer_selected(), 0);
        }

        #[test]
        fn metadata_reload_does_not_cancel_diagnostics_modal_task() {
            let mut state = sqlite_state_with_dsn("sqlite:///tmp/test.db");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            let diagnostics_run_id = state.sqlite_diagnostics.begin_core_fetch();
            state.modal.set_mode(InputMode::SqliteDiagnostics);
            let metadata_run_id = state.session.begin_metadata_refresh();

            let effects = dispatch_metadata(
                &mut state,
                &Action::MetadataLoaded {
                    dsn: "sqlite:///tmp/test.db".to_string(),
                    run_id: metadata_run_id,
                    metadata: make_metadata(vec![("public", "orders")]),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CancelTrackedTasks))
            );
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CancelSqliteDiagnostics))
            );
            assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
            assert!(state.sqlite_diagnostics.is_current_run(diagnostics_run_id));
            assert!(state.sqlite_diagnostics.snapshot().is_none());
        }

        #[test]
        fn table_still_exists_preserves_pagination_and_emits_refresh_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.query.pagination.reset_for_table("public", "users");

            // "orders" comes before "users" alphabetically, so "users" → index 1
            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = dispatch_metadata(&mut state, &action, Instant::now()).unwrap();

            assert_eq!(state.query.pagination.table(), "users");
            assert_eq!(state.ui.explorer_selected(), 1);
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

            assert_eq!(state.ui.explorer_selected(), 0);
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

    mod start_er_prefetch_all {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })
        }

        #[test]
        fn large_db_emits_resize_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(530)));

            let effects =
                dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now())
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

            let effects =
                dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now())
                    .into_effects()
                    .expect("reducer should handle action");

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ResizeCompletionCache { capacity: 500 }))
            );
        }

        #[test]
        fn very_large_db_uses_ceiling_capacity() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(10_001)));

            let effects =
                dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now())
                    .into_effects()
                    .expect("reducer should handle action");

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ResizeCompletionCache { capacity: 10_000 }))
            );
        }

        #[test]
        fn sets_fk_expanded_true() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(10)));

            dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now());

            assert!(state.er_preparation.fk_expanded());
        }

        #[test]
        fn process_queue_starts_prefetch_effects_without_action_redispatch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(2)));
            dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now());
            let run_id = state
                .sql_modal
                .active_prefetch_run_id()
                .expect("prefetch run");

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 2);
            assert!(
                effects
                    .iter()
                    .all(|effect| matches!(effect, Effect::PrefetchTableColumnsAndFks { .. }))
            );
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::DispatchActions(_)))
            );
        }

        #[test]
        fn process_empty_queue_returns_handled_without_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
            )
            .into_effects()
            .expect("empty prefetch queue should be handled");

            assert!(effects.is_empty());
            assert_eq!(state.sql_modal.active_prefetch_run_id(), Some(run_id));
            assert!(!state.sql_modal.has_pending_prefetch());
            assert_eq!(state.sql_modal.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_queue_without_dequeuing() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.sql_modal.begin_completion_prefetch();
            state
                .sql_modal
                .queue_table_prefetch("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
            )
            .into_effects()
            .expect("prefetch queue action should be handled");

            assert!(effects.is_empty());
            assert!(state.sql_modal.is_prefetch_queued("public.users"));
            assert_eq!(state.sql_modal.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_start_without_queueing() {
            let mut state = state_with_pending_mysql_probe();
            state.session.set_metadata(Some(make_metadata(2)));

            let effects =
                dispatch_metadata(&mut state, &Action::StartErPrefetchAll, Instant::now())
                    .into_effects()
                    .expect("prefetch action should be handled");

            assert!(effects.is_empty());
            assert!(state.sql_modal.active_prefetch_run_id().is_none());
            assert!(!state.sql_modal.has_pending_prefetch());
        }
    }

    mod start_er_prefetch_scoped {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None, false))
                .collect();
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })
        }

        #[test]
        fn second_call_while_running_is_ignored() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.sql_modal.begin_er_prefetch();
            state
                .er_preparation
                .queue_pending_table("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::StartErPrefetchScoped {
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            // In-progress prefetch must not be silently reset
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.users")
            );
            assert!(effects.is_empty());
        }

        #[test]
        fn only_selected_tables_in_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            let effects = dispatch_metadata(
                &mut state,
                &Action::StartErPrefetchScoped {
                    tables: tables.clone(),
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.sql_modal.is_prefetch_queued("public.users"));
            assert!(state.sql_modal.is_prefetch_queued("public.orders"));
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.users")
            );
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.users")
            );
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.orders")
            );
            assert!(!state.er_preparation.fk_expanded());
            assert_eq!(state.er_preparation.seed_tables(), tables);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
        }

        #[test]
        fn resizes_to_total_table_count_before_processing_scoped_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(560)));
            let tables: Vec<String> = (0..60).map(|i| format!("public.t{i}")).collect();

            let effects = dispatch_metadata(
                &mut state,
                &Action::StartErPrefetchScoped { tables },
                Instant::now(),
            )
            .into_effects()
            .expect("reducer should handle action");

            assert!(matches!(
                effects.as_slice(),
                [
                    Effect::ResizeCompletionCache { capacity: 560 },
                    Effect::SchedulePrefetchQueueProcessing { .. }
                ]
            ));
        }
    }

    mod start_completion_prefetch {
        use super::*;

        #[test]
        fn queues_only_referenced_tables_without_er_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            let effects = dispatch_metadata(
                &mut state,
                &Action::StartCompletionPrefetch { tables },
                Instant::now(),
            )
            .expect("completion prefetch should be handled");

            assert!(state.sql_modal.active_prefetch_run_id().is_some());
            assert!(!state.sql_modal.prefetch_tracks_er());
            assert!(state.sql_modal.is_prefetch_queued("public.users"));
            assert!(state.sql_modal.is_prefetch_queued("public.orders"));
            assert!(state.er_preparation.pending_tables().is_empty());
            assert!(effects.iter().any(|effect| {
                matches!(effect, Effect::SchedulePrefetchQueueProcessing { .. })
            }));
            assert!(
                effects
                    .iter()
                    .all(|effect| !matches!(effect, Effect::ResizeCompletionCache { .. }))
            );
        }

        #[test]
        fn cached_table_retriggers_sql_completion_without_er_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.modal.set_mode(InputMode::SqlModal);
            let run_id = state.sql_modal.begin_completion_prefetch();
            state
                .sql_modal
                .start_table_prefetch("public.users".to_string());

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
            .expect("cached detail should be handled");

            assert!(state.er_preparation.pending_tables().is_empty());
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::CacheTableInCompletionEngine { .. }))
            );
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::TriggerCompletion))
            );
        }

        #[test]
        fn er_prefetch_replaces_an_active_completion_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let completion_run_id = state.sql_modal.begin_completion_prefetch();
            state
                .sql_modal
                .start_table_prefetch("public.users".to_string());

            let effects = dispatch_metadata(
                &mut state,
                &Action::StartErPrefetchScoped {
                    tables: vec!["public.orders".to_string()],
                },
                Instant::now(),
            )
            .expect("ER prefetch should be handled");

            let er_run_id = state
                .sql_modal
                .active_prefetch_run_id()
                .expect("ER prefetch should have an active run");
            assert_ne!(completion_run_id, er_run_id);
            assert!(state.sql_modal.prefetch_tracks_er());
            assert!(!state.sql_modal.is_table_prefetching("public.users"));
            assert!(state.sql_modal.is_prefetch_queued("public.orders"));
            assert!(!state.sql_modal.is_prefetch_queued("public.users"));
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.orders")
            );
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::SchedulePrefetchQueueProcessing { run_id } if *run_id == er_run_id
            )));
        }
    }

    mod completion_check {
        use super::*;

        #[test]
        fn complete_not_fk_expanded_dispatches_expand() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_unexpanded();
            // pending and fetching are empty → is_complete() = true

            let effects = check_er_completion(&mut state);

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::ExtractFkNeighbors { run_id: action_run_id, .. }
                    if *action_run_id == run_id
            )));
        }

        #[test]
        fn complete_fk_expanded_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_expanded();

            let effects = check_er_completion(&mut state);

            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn stale_expand_does_not_start_neighbor_extraction() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let stale_run_id = state.sql_modal.begin_er_prefetch();
            let current_run_id = state.sql_modal.begin_er_prefetch();

            let effects = dispatch_metadata(
                &mut state,
                &Action::ExpandPrefetchWithFkNeighbors {
                    run_id: stale_run_id,
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(
                state.sql_modal.active_prefetch_run_id(),
                Some(current_run_id)
            );
            assert!(effects.is_empty());
        }

        #[test]
        fn pending_mysql_probe_rejects_neighbor_expansion() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_unexpanded();

            let effects = dispatch_metadata(
                &mut state,
                &Action::ExpandPrefetchWithFkNeighbors { run_id },
                Instant::now(),
            )
            .into_effects()
            .expect("neighbor expansion action should be handled");

            assert!(effects.is_empty());
            assert!(state.er_preparation.is_waiting());
            assert!(!state.er_preparation.fk_expanded());
        }
    }

    mod fk_neighbors_discovered {
        use super::prefetch::MAX_PREFETCH_RETRIES;
        use super::*;

        #[test]
        fn empty_neighbors_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id,
                    tables: vec![],
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.fk_expanded());
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn non_empty_neighbors_adds_to_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id,
                    tables: vec!["public.posts".to_string(), "public.tags".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            assert!(state.er_preparation.fk_expanded());
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.posts")
            );
            assert!(
                state
                    .er_preparation
                    .pending_tables()
                    .contains("public.tags")
            );
            assert!(state.sql_modal.is_prefetch_queued("public.posts"));
            assert!(state.sql_modal.is_prefetch_queued("public.tags"));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
        }

        #[test]
        fn stale_neighbors_without_active_run_do_not_mutate_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.er_preparation.mark_waiting_for_test();

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id: 1,
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            assert!(!state.er_preparation.fk_expanded());
            assert!(state.er_preparation.pending_tables().is_empty());
            assert!(!state.sql_modal.has_pending_prefetch());
            assert!(effects.is_empty());
        }

        #[test]
        fn pending_mysql_probe_rejects_discovered_neighbors_without_queueing() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_unexpanded();

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id,
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .into_effects()
            .expect("discovered neighbors action should be handled");

            assert!(effects.is_empty());
            assert!(!state.er_preparation.fk_expanded());
            assert!(
                !state
                    .er_preparation
                    .pending_tables()
                    .contains("public.posts")
            );
            assert!(!state.sql_modal.is_prefetch_queued("public.posts"));
        }

        #[test]
        fn stale_neighbors_from_previous_run_do_not_mutate_current_run() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let stale_run_id = state.sql_modal.begin_er_prefetch();
            let current_run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();

            let effects = dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id: stale_run_id,
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
            )
            .unwrap();

            assert_eq!(
                state.sql_modal.active_prefetch_run_id(),
                Some(current_run_id)
            );
            assert!(!state.er_preparation.fk_expanded());
            assert!(state.er_preparation.pending_tables().is_empty());
            assert!(effects.is_empty());
        }

        #[test]
        fn duplicate_neighbors_are_not_requeued() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state
                .er_preparation
                .queue_pending_table("public.posts".to_string());
            state
                .sql_modal
                .queue_table_prefetch("public.posts".to_string());
            state
                .sql_modal
                .start_table_prefetch("public.tags".to_string());

            dispatch_metadata(
                &mut state,
                &Action::FkNeighborsDiscovered {
                    run_id,
                    tables: vec![
                        "public.posts".to_string(),
                        "public.tags".to_string(),
                        "public.comments".to_string(),
                    ],
                },
                Instant::now(),
            );

            assert!(state.sql_modal.is_prefetch_queued("public.posts"));
            assert!(state.sql_modal.is_prefetch_queued("public.comments"));
            assert!(!state.sql_modal.is_prefetch_queued("public.tags"));
            assert_eq!(
                state.sql_modal.take_next_prefetch(),
                Some("public.posts".to_string())
            );
            assert_eq!(
                state.sql_modal.take_next_prefetch(),
                Some("public.comments".to_string())
            );
            assert!(!state.sql_modal.has_pending_prefetch());
        }

        #[test]
        fn phase2_table_retry_limit_triggers_completion() {
            // All Phase 2 tables fail → completion must still fire
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.sql_modal.begin_er_prefetch();
            state.er_preparation.mark_waiting_for_test();
            state.er_preparation.mark_fk_expanded();
            let neighbor = "public.posts".to_string();
            state.er_preparation.queue_pending_table(neighbor.clone());
            state.sql_modal.fail_table_prefetch(
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

            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::WriteErFailureLog { .. }))
            );
        }
    }
}
