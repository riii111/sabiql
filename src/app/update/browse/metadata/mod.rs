mod er_neighbors;
pub(in crate::update) mod loading;
pub(in crate::update) mod prefetch;
pub(in crate::update) mod table_detail;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::update::action::Action;

pub(super) fn check_er_completion(state: &mut AppState) -> Vec<Effect> {
    if state.er_preparation.status() != ErStatus::Waiting || !state.table_prefetch.is_complete() {
        return vec![];
    }

    if !state.er_preparation.fk_expanded() {
        let Some(run_id) = state.table_prefetch.active_prefetch_run_id() else {
            return vec![];
        };
        return er_neighbors::expand_prefetch_with_fk_neighbors(state, run_id);
    }

    if !state.table_prefetch.has_failures() {
        state.er_preparation.mark_idle();
        return vec![Effect::DispatchActions(vec![Action::ErGenerateFromCache])];
    }

    state.er_preparation.mark_idle();
    let failed_data: Vec<(String, String)> = state.table_prefetch.failed_table_errors();
    state.messages.set_error(format!(
        "ER failed: {} table(s) failed. 'e' to retry.",
        failed_data.len()
    ));
    vec![Effect::WriteErFailureLog {
        failed_tables: failed_data,
    }]
}

#[cfg(test)]
mod tests {
    use crate::test_support;
    use crate::test_support::endpoint;

    use super::*;
    use crate::cmd::effect::Effect;
    use crate::domain::{ConnectionId, Table};
    use crate::model::app_state::AppState;
    use crate::model::browse::session::TableDetailState;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::table_prefetch::FailedPrefetchEntry;
    use crate::ports::outbound::DbOperationError;
    use crate::services::AppServices;
    use crate::update::action::Action;
    use crate::update::reducer::route;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::new(),
            "postgres",
            &endpoint::postgres(dsn),
        );
        state
    }

    fn sqlite_state_with_dsn(dsn: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state
            .session
            .activate_connection(&ConnectionId::new(), "sqlite", &endpoint::sqlite(dsn));
        state
    }

    fn state_with_pending_mysql_probe() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::from_string("mysql-current"),
            "mysql-current",
            &endpoint::mysql("mysql://localhost/current", Some("current")),
        );
        let _ = state.session.begin_mysql_connection_probe(
            &ConnectionId::from_string("mysql-target"),
            "mysql-target",
            &endpoint::mysql("mysql://localhost/target", Some("target")),
        );
        state
    }

    fn empty_table(schema: &str, name: &str) -> Box<Table> {
        Box::new(test_support::table::minimal(schema, name))
    }

    fn cache_keeps_every_table(state: &mut AppState, table_count: usize) -> bool {
        for index in 0..table_count {
            state
                .table_detail_cache
                .insert(format!("public.t{index}"), *empty_table("public", "t"));
        }
        state.table_detail_cache.contains("public.t0")
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
                )];
                metadata
            })
        }

        #[test]
        fn stale_metadata_loaded_does_not_replace_current_state() {
            let mut state = state_with_dsn("postgres://localhost/new");
            let stale_run_id = state.session.begin_metadata_refresh();
            let _current_run_id = state.session.begin_metadata_refresh();

            let effects = route(
                &mut state,
                Action::MetadataLoaded {
                    run_id: stale_run_id,
                    metadata: metadata_with_users(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.session.metadata().is_none());
        }

        #[test]
        fn stale_table_detail_loaded_does_not_replace_current_detail() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.session.begin_table_detail_run();
            let current_generation = state.session.selection_generation();
            let _ = state.session.begin_table_detail_run();

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    outcome: Ok(empty_table("public", "users")),
                    generation: current_generation,
                },
                Instant::now(),
                &AppServices::stub(),
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

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    outcome: Err(DbOperationError::PermissionDenied("denied".to_string())),
                    generation,
                },
                Instant::now(),
                &AppServices::stub(),
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

            route(
                &mut state,
                Action::MetadataFailed {
                    run_id: metadata_run_id,
                    error: DbOperationError::PermissionDenied("denied".to_string()),
                },
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(matches!(
                state.session.table_detail_state(),
                TableDetailState::Error(_)
            ));
            assert!(!state.session.is_current_table_detail_run(detail_run_id));

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id: detail_run_id,
                    outcome: Ok(empty_table("public", "users")),
                    generation,
                },
                Instant::now(),
                &AppServices::stub(),
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

            route(
                &mut state,
                Action::TableDetailLoaded {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id: current_run_id,
                    outcome: Err(DbOperationError::PermissionDenied("denied".to_string())),
                    generation: stale_generation,
                },
                Instant::now(),
                &AppServices::stub(),
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
            let old_run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id: old_run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.has_pending_prefetch());
            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert_eq!(state.table_prefetch.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn mysql_reload_metadata_starts_fetch() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "mysql",
                &endpoint::mysql(
                    "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
                    Some("app"),
                ),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            let effects = route(
                &mut state,
                Action::ReloadMetadata,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );
            assert!(state.messages.last_error().is_none());
        }
    }

    mod prefetch_table_detail {
        use super::prefetch::MAX_PREFETCH_RETRIES;
        use super::*;

        #[test]
        fn backoff_table_requeued_at_tail_with_process_effect() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            let queued = "public.orders".to_string();
            state.table_prefetch.queue_table_prefetch(queued.clone());
            // Insert a recently failed entry (retry_count=1, just failed)
            state.table_prefetch.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            // Should be re-queued at tail
            assert_eq!(state.table_prefetch.take_next_prefetch(), Some(queued));
            assert_eq!(state.table_prefetch.take_next_prefetch(), Some(qualified));
            assert!(!state.table_prefetch.has_pending_prefetch());
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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            let failed_at = Instant::now();
            let now = failed_at.checked_add(Duration::from_secs(1)).unwrap();
            state.table_prefetch.fail_table_prefetch(
                qualified,
                FailedPrefetchEntry {
                    failed_at,
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                now,
                &AppServices::stub(),
            );

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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.table_prefetch.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );
            state.table_prefetch.queue_table_prefetch(qualified.clone());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                effects
                    .iter()
                    .filter(|effect| matches!(effect, Effect::DelayedProcessPrefetchQueue { .. }))
                    .count(),
                1
            );
            assert_eq!(state.table_prefetch.take_next_prefetch(), Some(qualified));
            assert!(!state.table_prefetch.has_pending_prefetch());
        }

        #[test]
        fn no_dsn_requeues_without_marking_in_flight() {
            let mut state = AppState::new("test".to_string());
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.table_prefetch.queue_table_prefetch(qualified.clone());

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.is_prefetch_queued(&qualified));
            assert!(!state.table_prefetch.is_table_prefetching(&qualified));
        }

        #[test]
        fn pending_mysql_probe_rejects_direct_prefetch_without_starting_it() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.table_prefetch.begin_completion_prefetch();

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(!state.table_prefetch.is_table_prefetching("public.users"));
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_completion_without_cache_mutation() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.table_prefetch.begin_completion_prefetch();
            state
                .table_prefetch
                .start_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::TableDetailCached {
                    endpoint: endpoint::mysql("mysql://localhost/current", Some("current")),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: empty_table("public", "users"),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.is_table_prefetching("public.users"));
        }

        #[test]
        fn retry_limit_exceeded_gives_up_and_keeps_terminal_failure() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.table_prefetch.queue_table_prefetch(qualified.clone());
            state.table_prefetch.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(!state.table_prefetch.is_prefetch_queued(&qualified));
            assert!(state.table_prefetch.has_failures());
        }

        #[test]
        fn retry_limit_exceeded_as_last_table_triggers_er_completion() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_fk_expanded();
            let qualified = "public.users".to_string();
            // Only table remaining; retry limit exceeded
            state.table_prefetch.queue_table_prefetch(qualified.clone());
            state.table_prefetch.fail_table_prefetch(
                qualified,
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_fk_expanded();
            let failed = "public.users".to_string();
            let remaining = "public.posts".to_string();
            // users exhausted retries; posts still awaiting in queue
            state.table_prefetch.queue_table_prefetch(failed.clone());
            state.table_prefetch.queue_table_prefetch(remaining);
            state.table_prefetch.fail_table_prefetch(
                failed,
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
        }

        #[test]
        fn process_queue_keeps_later_table_owned_after_terminal_failure() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_fk_expanded();
            let failed = "public.users".to_string();
            let remaining = "public.posts".to_string();
            state.table_prefetch.fail_table_prefetch(
                failed.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: MAX_PREFETCH_RETRIES,
                },
            );
            state.table_prefetch.queue_table_prefetch(failed);
            state.table_prefetch.queue_table_prefetch(remaining.clone());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.table_prefetch.is_table_prefetching(&remaining));
            assert_eq!(state.er_preparation.status(), ErStatus::Waiting);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::WriteErFailureLog { .. }))
            );
            assert!(!effects.iter().any(|effect| matches!(
                effect,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|action| matches!(action, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn expired_backoff_proceeds_normally() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            // Failed 10 seconds ago with retry_count=1 (backoff = 2s, already expired)
            state.table_prefetch.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            // Should proceed to fetching
            assert!(state.table_prefetch.is_table_prefetching(&qualified));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::PrefetchTableColumnsAndFks { .. }))
            );
        }
    }

    mod cached_prefetch {
        use super::*;

        fn schedule_count(effects: &[Effect]) -> usize {
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::SchedulePrefetchQueueProcessing { .. }))
                .count()
        }

        fn fetch_count(effects: &[Effect]) -> usize {
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::PrefetchTableColumnsAndFks { .. }))
                .count()
        }

        fn er_state_with_queue(cached: &[&str], uncached: &[&str]) -> (AppState, u64) {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            for name in cached {
                state
                    .table_detail_cache
                    .insert(format!("public.{name}"), *empty_table("public", name));
            }
            for name in cached.iter().chain(uncached) {
                state
                    .table_prefetch
                    .queue_table_prefetch(format!("public.{name}"));
            }
            (state, run_id)
        }

        #[test]
        fn all_cached_batch_schedules_once() {
            let (mut state, run_id) =
                er_state_with_queue(&["t0", "t1", "t2", "t3", "t4", "t5"], &[]);

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(fetch_count(&effects), 0);
            assert_eq!(schedule_count(&effects), 1);
            assert_eq!(state.table_prefetch.pending_prefetch_count(), 2);
        }

        #[test]
        fn mixed_batch_schedules_once_when_slots_remain() {
            let (mut state, run_id) = er_state_with_queue(&["t0", "t1"], &["t2", "t3", "t4"]);

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(fetch_count(&effects), 2);
            assert_eq!(schedule_count(&effects), 1);
        }

        #[test]
        fn batch_without_cache_hits_keeps_existing_effects() {
            let (mut state, run_id) = er_state_with_queue(&[], &["t0", "t1", "t2", "t3", "t4"]);

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(fetch_count(&effects), 4);
            assert_eq!(schedule_count(&effects), 0);
        }

        #[test]
        fn full_slots_do_not_schedule() {
            let (mut state, run_id) = er_state_with_queue(&["t0"], &[]);
            for name in ["a", "b", "c", "d"] {
                state
                    .table_prefetch
                    .start_table_prefetch(format!("public.{name}"));
            }

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.is_prefetch_queued("public.t0"));
        }

        #[test]
        fn missing_connection_defers_without_scheduling() {
            let mut state = AppState::new("test".to_string());
            let run_id = state.table_prefetch.begin_er_prefetch();
            state
                .table_detail_cache
                .insert("public.t0".to_string(), *empty_table("public", "t0"));
            state
                .table_prefetch
                .queue_table_prefetch("public.t0".to_string());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.is_prefetch_queued("public.t0"));
        }

        #[test]
        fn backoff_retry_keeps_delayed_schedule_only() {
            let (mut state, run_id) = er_state_with_queue(&[], &["t0"]);
            state.table_prefetch.fail_table_prefetch(
                "public.t0".to_string(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );
            state
                .table_prefetch
                .queue_table_prefetch("public.t0".to_string());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(schedule_count(&effects), 0);
            assert!(matches!(
                effects.as_slice(),
                [Effect::DelayedProcessPrefetchQueue { .. }]
            ));
        }

        #[test]
        fn direct_request_fetches_uncached_table_beyond_slot_limit() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_completion_prefetch();
            for name in ["a", "b", "c", "d"] {
                state
                    .table_prefetch
                    .start_table_prefetch(format!("public.{name}"));
            }

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(fetch_count(&effects), 1);
        }

        #[test]
        fn direct_request_completes_cached_table_without_fetching() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.modal.set_mode(InputMode::SqlModal);
            let run_id = state.table_prefetch.begin_completion_prefetch();
            state
                .table_detail_cache
                .insert("public.users".to_string(), *empty_table("public", "users"));

            let effects = route(
                &mut state,
                Action::PrefetchTableDetail {
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(fetch_count(&effects), 0);
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::DispatchActions(actions)
                    if matches!(actions.as_slice(), [Action::CompletionRequest])
            )));
        }
    }

    mod table_detail_cache_failed {
        use super::*;

        #[test]
        fn increments_retry_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.table_prefetch.fail_table_prefetch(
                qualified.clone(),
                FailedPrefetchEntry {
                    failed_at: Instant::now().checked_sub(Duration::from_mins(1)).unwrap(),
                    error: "old error".to_string(),
                    retry_count: 1,
                },
            );
            state.table_prefetch.start_table_prefetch(qualified.clone());

            assert!(state.table_prefetch.is_table_prefetching(&qualified));

            let now = Instant::now();
            route(
                &mut state,
                Action::TableDetailCacheFailed {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::QueryFailed("new error".to_string()),
                },
                now,
                &AppServices::stub(),
            );

            let entry = state.table_prefetch.failed_prefetch(&qualified).unwrap();
            assert_eq!(entry.retry_count, 2);
            assert!(!state.table_prefetch.is_table_prefetching(&qualified));
            assert_eq!(
                entry.error,
                "Query failed: new error. Review the database error details and SQL."
            );
        }

        #[test]
        fn failure_requeues_table_for_retry_with_delayed_process() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let qualified = "public.users".to_string();
            state.table_prefetch.start_table_prefetch(qualified.clone());

            let effects = route(
                &mut state,
                Action::TableDetailCacheFailed {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.table_prefetch.is_prefetch_queued(&qualified));
            assert!(!state.table_prefetch.is_table_prefetching(&qualified));
            assert_eq!(
                state
                    .table_prefetch
                    .failed_prefetch(&qualified)
                    .unwrap()
                    .retry_count,
                1
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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let failed = "public.users".to_string();
            let queued = "public.posts".to_string();
            state.table_prefetch.start_table_prefetch(failed);
            state.table_prefetch.queue_table_prefetch(queued);

            let effects = route(
                &mut state,
                Action::TableDetailCacheFailed {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
                &AppServices::stub(),
            );

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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_fk_expanded();
            let qualified = "public.users".to_string();
            state.table_prefetch.start_table_prefetch(qualified.clone());

            route(
                &mut state,
                Action::TableDetailCacheFailed {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    error: DbOperationError::Timeout("timed out".to_string()),
                },
                Instant::now(),
                &AppServices::stub(),
            );
            let _ = state.table_prefetch.take_next_prefetch();
            state.table_prefetch.queue_table_prefetch(qualified);

            let effects = route(
                &mut state,
                Action::TableDetailCached {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: empty_table("public", "users"),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(!state.table_prefetch.has_failures());
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
                        TableSummary::new(schema.to_string(), name.to_string(), None)
                    })
                    .collect();
                metadata
            })
        }

        fn metadata_loaded_action(state: &mut AppState, metadata: Arc<DatabaseMetadata>) -> Action {
            let run_id = state.session.begin_metadata_refresh();
            Action::MetadataLoaded { run_id, metadata }
        }

        #[test]
        fn table_disappeared_clears_selection_and_cancels_before_effective_user_fetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);

            let metadata = make_metadata(vec![("public", "orders")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(matches!(
                effects.as_slice(),
                [
                    Effect::CancelTrackedTasks,
                    Effect::FetchEffectiveUser { .. }
                ]
            ));

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

            let effects = route(
                &mut state,
                Action::MetadataLoaded {
                    run_id: metadata_run_id,
                    metadata: make_metadata(vec![("public", "orders")]),
                },
                Instant::now(),
                &AppServices::stub(),
            );

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
            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert_eq!(state.query.pagination.table(), "users");
            assert_eq!(state.ui.explorer_selected(), 1);
            assert!(matches!(
                effects.as_slice(),
                [Effect::FetchEffectiveUser { .. }, Effect::ExecutePreview { table, .. }, Effect::FetchTableDetail { table: detail_table, .. }]
                    if table == "users" && detail_table == "users"
            ));
        }

        #[test]
        fn no_table_selected_defaults_to_first() {
            let mut state = state_with_dsn("postgres://localhost/test");

            let metadata = make_metadata(vec![("public", "orders"), ("public", "users")]);
            let action = metadata_loaded_action(&mut state, metadata);
            let effects = route(&mut state, action, Instant::now(), &AppServices::stub());

            assert!(matches!(
                effects.as_slice(),
                [Effect::FetchEffectiveUser { .. }]
            ));

            assert_eq!(state.ui.explorer_selected(), 0);
        }
    }

    mod start_er_prefetch_all {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None))
                .collect();
            Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })
        }

        #[test]
        fn large_db_grows_cache_to_table_count() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(530)));

            route(
                &mut state,
                Action::StartErPrefetchAll,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(cache_keeps_every_table(&mut state, 530));
        }

        #[test]
        fn sets_fk_expanded_true() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(10)));

            route(
                &mut state,
                Action::StartErPrefetchAll,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.er_preparation.fk_expanded());
        }

        #[test]
        fn process_queue_starts_prefetch_effects_without_action_redispatch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.session.set_metadata(Some(make_metadata(2)));
            route(
                &mut state,
                Action::StartErPrefetchAll,
                Instant::now(),
                &AppServices::stub(),
            );
            let run_id = state
                .table_prefetch
                .active_prefetch_run_id()
                .expect("prefetch run");

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

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
        fn process_empty_queue_returns_no_effects() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert_eq!(state.table_prefetch.active_prefetch_run_id(), Some(run_id));
            assert!(!state.table_prefetch.has_pending_prefetch());
            assert_eq!(state.table_prefetch.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_queue_without_dequeuing() {
            let mut state = state_with_pending_mysql_probe();
            let run_id = state.table_prefetch.begin_completion_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::ProcessPrefetchQueue { run_id },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert_eq!(state.table_prefetch.prefetch_in_flight_count(), 0);
        }

        #[test]
        fn pending_mysql_probe_rejects_prefetch_start_without_queueing() {
            let mut state = state_with_pending_mysql_probe();
            state.session.set_metadata(Some(make_metadata(2)));

            let effects = route(
                &mut state,
                Action::StartErPrefetchAll,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert!(!state.table_prefetch.has_pending_prefetch());
        }
    }

    mod start_er_prefetch_scoped {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn make_metadata(table_count: usize) -> Arc<DatabaseMetadata> {
            let tables: Vec<TableSummary> = (0..table_count)
                .map(|i| TableSummary::new(format!("t{i}"), "public".to_string(), None))
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
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::StartErPrefetchScoped {
                    tables: vec!["public.posts".to_string()],
                },
                Instant::now(),
                &AppServices::stub(),
            );

            // In-progress prefetch must not be silently reset
            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert!(effects.is_empty());
        }

        #[test]
        fn only_selected_tables_in_queue() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            let effects = route(
                &mut state,
                Action::StartErPrefetchScoped {
                    tables: tables.clone(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert!(state.table_prefetch.is_prefetch_queued("public.orders"));
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

            let effects = route(
                &mut state,
                Action::StartErPrefetchScoped { tables },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(matches!(
                effects.as_slice(),
                [Effect::SchedulePrefetchQueueProcessing { .. }]
            ));
            assert!(cache_keeps_every_table(&mut state, 560));
        }
    }

    mod start_completion_prefetch {
        use super::*;

        #[test]
        fn queues_only_referenced_tables_without_er_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            let effects = route(
                &mut state,
                Action::StartCompletionPrefetch { tables },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.table_prefetch.active_prefetch_run_id().is_some());
            assert!(!state.table_prefetch.prefetch_tracks_er());
            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert!(state.table_prefetch.is_prefetch_queued("public.orders"));
            assert!(effects.iter().any(|effect| {
                matches!(effect, Effect::SchedulePrefetchQueueProcessing { .. })
            }));
            assert!(!cache_keeps_every_table(&mut state, 501));
        }

        #[test]
        fn fetched_table_retriggers_sql_completion_without_er_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            state.modal.set_mode(InputMode::SqlModal);
            let run_id = state.table_prefetch.begin_completion_prefetch();
            state
                .table_prefetch
                .start_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::TableDetailCached {
                    endpoint: endpoint::postgres("postgres://localhost/test"),
                    run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: empty_table("public", "users"),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.table_detail_cache.contains("public.users"));
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::DispatchActions(actions)
                    if matches!(actions.as_slice(), [Action::CompletionRequest])
            )));
        }

        #[test]
        fn er_prefetch_replaces_an_active_completion_prefetch() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let completion_run_id = state.table_prefetch.begin_completion_prefetch();
            state
                .table_prefetch
                .start_table_prefetch("public.users".to_string());

            let effects = route(
                &mut state,
                Action::StartErPrefetchScoped {
                    tables: vec!["public.orders".to_string()],
                },
                Instant::now(),
                &AppServices::stub(),
            );

            let er_run_id = state
                .table_prefetch
                .active_prefetch_run_id()
                .expect("ER prefetch should have an active run");
            assert_ne!(completion_run_id, er_run_id);
            assert!(state.table_prefetch.prefetch_tracks_er());
            assert!(!state.table_prefetch.is_table_prefetching("public.users"));
            assert!(state.table_prefetch.is_prefetch_queued("public.orders"));
            assert!(!state.table_prefetch.is_prefetch_queued("public.users"));
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::SchedulePrefetchQueueProcessing { run_id } if *run_id == er_run_id
            )));
        }
    }

    mod completion_check {
        use super::*;

        #[test]
        fn complete_without_fk_neighbors_expands_and_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();

            let effects = check_er_completion(&mut state);

            assert!(state.er_preparation.fk_expanded());
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::DispatchActions(actions)
                    if actions.iter().any(|a| matches!(a, Action::ErGenerateFromCache))
            )));
        }

        #[test]
        fn complete_with_uncached_fk_neighbor_queues_it() {
            use crate::domain::{FkAction, ForeignKey};

            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state
                .er_preparation
                .begin_scoped_prefetch(["public.orders".to_string()]);
            let mut orders = test_support::table::minimal("public", "orders");
            orders.foreign_keys = vec![ForeignKey {
                name: "orders_user_fk".to_string(),
                from_schema: "public".to_string(),
                from_table: "orders".to_string(),
                from_columns: vec!["user_id".to_string()],
                to_schema: "public".to_string(),
                to_table: "users".to_string(),
                to_columns: vec!["id".to_string()],
                on_delete: FkAction::NoAction,
                on_update: FkAction::NoAction,
                reference_resolved: true,
            }];
            state
                .table_detail_cache
                .insert("public.orders".to_string(), orders);

            let effects = check_er_completion(&mut state);

            assert!(state.er_preparation.fk_expanded());
            assert!(state.table_prefetch.is_prefetch_queued("public.users"));
            assert!(effects.iter().any(|e| matches!(
                e,
                Effect::SchedulePrefetchQueueProcessing { run_id: scheduled } if *scheduled == run_id
            )));
        }

        #[test]
        fn complete_fk_expanded_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_fk_expanded();

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

        #[test]
        fn empty_neighbors_dispatches_generate() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();

            let effects = er_neighbors::queue_fk_neighbors(&mut state, run_id, &[]);

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
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();

            let effects = er_neighbors::queue_fk_neighbors(
                &mut state,
                run_id,
                &["public.posts".to_string(), "public.tags".to_string()],
            );

            assert!(state.er_preparation.fk_expanded());
            assert!(state.table_prefetch.is_prefetch_queued("public.posts"));
            assert!(state.table_prefetch.is_prefetch_queued("public.tags"));
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::SchedulePrefetchQueueProcessing { .. }))
            );
        }

        #[test]
        fn stale_neighbors_without_active_run_do_not_mutate_state() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let _ = state.er_preparation.start_waiting_run();

            let effects =
                er_neighbors::queue_fk_neighbors(&mut state, 1, &["public.posts".to_string()]);

            assert!(!state.er_preparation.fk_expanded());
            assert!(!state.table_prefetch.has_pending_prefetch());
            assert!(effects.is_empty());
        }

        #[test]
        fn stale_neighbors_from_previous_run_do_not_mutate_current_run() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let stale_run_id = state.table_prefetch.begin_er_prefetch();
            let current_run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();

            let effects = er_neighbors::queue_fk_neighbors(
                &mut state,
                stale_run_id,
                &["public.posts".to_string()],
            );

            assert_eq!(
                state.table_prefetch.active_prefetch_run_id(),
                Some(current_run_id)
            );
            assert!(!state.er_preparation.fk_expanded());
            assert!(effects.is_empty());
        }

        #[test]
        fn duplicate_neighbors_are_not_requeued() {
            let mut state = state_with_dsn("postgres://localhost/test");
            let run_id = state.table_prefetch.begin_er_prefetch();
            let _ = state.er_preparation.start_waiting_run();
            state
                .table_prefetch
                .queue_table_prefetch("public.posts".to_string());
            state
                .table_prefetch
                .queue_table_prefetch("public.posts".to_string());
            state
                .table_prefetch
                .start_table_prefetch("public.tags".to_string());

            er_neighbors::queue_fk_neighbors(
                &mut state,
                run_id,
                &[
                    "public.posts".to_string(),
                    "public.tags".to_string(),
                    "public.comments".to_string(),
                ],
            );

            assert!(state.table_prefetch.is_prefetch_queued("public.posts"));
            assert!(state.table_prefetch.is_prefetch_queued("public.comments"));
            assert!(!state.table_prefetch.is_prefetch_queued("public.tags"));
            assert_eq!(
                state.table_prefetch.take_next_prefetch(),
                Some("public.posts".to_string())
            );
            assert_eq!(
                state.table_prefetch.take_next_prefetch(),
                Some("public.comments".to_string())
            );
            assert!(!state.table_prefetch.has_pending_prefetch());
        }
    }
}
