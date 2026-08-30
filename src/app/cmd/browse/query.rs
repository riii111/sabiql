use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::domain::command_tag::CommandTag;
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope, QueryResultStatus};
use crate::domain::{DatabaseDiagnostic, DatabaseType};
use crate::domain::{
    mysql_explain_plan_text_from_result, postgres_explain_plan_text_from_result,
    sqlite_explain_query_plan_text_from_result,
};
use crate::model::app_state::AppState;
use crate::policy::mask_password;
use crate::ports::outbound::{
    CachedResultExporter, DbOperationError, QueryExecutor, QueryHistoryStore,
};
use crate::update::action::{Action, QueryCompletionContext, QueryFailureContext};

fn mask_mysql_diagnostics(diagnostics: &mut [DatabaseDiagnostic]) {
    for diagnostic in diagnostics {
        diagnostic.message = mask_password(&diagnostic.message);
    }
}

fn epoch_days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn utc_now_iso8601() -> String {
    let now_sys = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now_sys.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (y, m, d) = epoch_days_to_ymd(days as i64);
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn spawn_query_history_append(
    query_history_store: Arc<dyn QueryHistoryStore>,
    project_name: String,
    scope: QueryHistoryScope,
    query: String,
    result_status: QueryResultStatus,
    affected_rows: Option<u64>,
) {
    let entry = QueryHistoryEntry::new_with_database(
        query,
        utc_now_iso8601(),
        scope.connection_id.clone(),
        scope.database.clone(),
        result_status,
        affected_rows,
    );
    tokio::spawn(async move {
        let _ = query_history_store
            .append(&project_name, &scope, &entry)
            .await;
    });
}

pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_executor: &Arc<dyn QueryExecutor>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
    cached_result_exporter: &Arc<dyn CachedResultExporter>,
    query_tasks: &SingleTaskOwner,
    state: &AppState,
) {
    match effect {
        Effect::ExecutePreview {
            dsn,
            schema,
            table,
            generation,
            run_id,
            limit,
            offset,
            target_page,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            query_tasks
                .replace(async move {
                    match executor
                        .execute_preview(&dsn, &schema, &table, limit, offset)
                        .await
                    {
                        Ok(result) => {
                            tx.send(Action::QueryCompleted {
                                run_id,
                                result: Arc::new(result),
                                context: QueryCompletionContext::Preview {
                                    generation,
                                    target_page,
                                },
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            tx.send(Action::QueryFailed {
                                run_id,
                                error: e,
                                context: QueryFailureContext::Preview { generation },
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ExecuteExplain {
            dsn,
            database_type,
            database_generation,
            run_id,
            query,
            source_query,
            is_analyze,
            access_mode,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            query_tasks
                .replace(async move {
                    match executor.execute_adhoc(&dsn, &query, access_mode).await {
                        Ok(result) => {
                            let plan_result = match database_type {
                                DatabaseType::SQLite => sqlite_explain_query_plan_text_from_result(
                                    &result,
                                )
                                .map_err(|error| DbOperationError::QueryFailed(error.to_string())),
                                DatabaseType::PostgreSQL => {
                                    Ok(postgres_explain_plan_text_from_result(&result))
                                }
                                DatabaseType::MySQL => {
                                    Ok(mysql_explain_plan_text_from_result(&result))
                                }
                            };
                            match plan_result {
                                Ok(plan_text) => {
                                    tx.send(Action::ExplainCompleted {
                                        dsn,
                                        database_type,
                                        database_generation,
                                        run_id,
                                        query: source_query,
                                        plan_text,
                                        is_analyze,
                                        execution_time_ms: result.execution_time_ms,
                                    })
                                    .await
                                    .ok();
                                }
                                Err(error) => {
                                    tx.send(Action::ExplainFailed {
                                        dsn,
                                        database_generation,
                                        run_id,
                                        error,
                                        is_analyze,
                                    })
                                    .await
                                    .ok();
                                }
                            }
                        }
                        Err(e) => {
                            tx.send(Action::ExplainFailed {
                                dsn,
                                database_generation,
                                run_id,
                                error: e,
                                is_analyze,
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ExecuteAdhoc {
            dsn,
            run_id,
            query,
            access_mode,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let history_store = Arc::clone(query_history_store);
            let project = state.runtime.project_name().to_string();
            let history_scope = state.session.query_history_scope();
            query_tasks
                .replace(async move {
                    let result = executor.execute_adhoc(&dsn, &query, access_mode).await;
                    match result {
                        Ok(result) => {
                            let mut result = result;
                            mask_mysql_diagnostics(&mut result.mysql_diagnostics);
                            if let Some(scope) = history_scope {
                                let rows = result
                                    .command_tag
                                    .as_ref()
                                    .and_then(CommandTag::affected_rows);
                                spawn_query_history_append(
                                    history_store,
                                    project,
                                    scope,
                                    query,
                                    QueryResultStatus::Success,
                                    rows,
                                );
                            }
                            tx.send(Action::QueryCompleted {
                                run_id,
                                result: Arc::new(result),
                                context: QueryCompletionContext::Adhoc,
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            if let Some(scope) = history_scope {
                                spawn_query_history_append(
                                    history_store,
                                    project,
                                    scope,
                                    query,
                                    QueryResultStatus::Failed,
                                    None,
                                );
                            }
                            tx.send(Action::QueryFailed {
                                run_id,
                                error: e,
                                context: QueryFailureContext::Adhoc,
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ExecuteWrite {
            dsn,
            run_id,
            query,
            access_mode,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let history_store = Arc::clone(query_history_store);
            let project = state.runtime.project_name().to_string();
            let history_scope = state.session.query_history_scope();

            query_tasks
                .replace(async move {
                    match executor.execute_write(&dsn, &query, access_mode).await {
                        Ok(result) => {
                            let mut result = result;
                            mask_mysql_diagnostics(&mut result.diagnostics);
                            if let Some(scope) = history_scope {
                                spawn_query_history_append(
                                    history_store,
                                    project,
                                    scope,
                                    query,
                                    QueryResultStatus::Success,
                                    Some(result.affected_rows as u64),
                                );
                            }
                            tx.send(Action::ExecuteWriteSucceeded {
                                dsn,
                                run_id,
                                affected_rows: result.affected_rows,
                                diagnostics: result.diagnostics,
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            if let Some(scope) = history_scope {
                                spawn_query_history_append(
                                    history_store,
                                    project,
                                    scope,
                                    query,
                                    QueryResultStatus::Failed,
                                    None,
                                );
                            }
                            tx.send(Action::ExecuteWriteFailed {
                                dsn,
                                run_id,
                                error: e,
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ExportCsv {
            dsn,
            run_id,
            query,
            file_name,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let export_dsn = dsn.clone();

            query_tasks
                .replace(async move {
                    let result = executor
                        .export_to_csv(&export_dsn, &query, &file_name)
                        .await;
                    match result {
                        Ok(path) => {
                            tx.send(Action::CsvExportSucceeded {
                                dsn,
                                run_id,
                                path: path.display().to_string(),
                                row_count: None,
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            tx.send(Action::CsvExportFailed {
                                dsn,
                                run_id,
                                error: e,
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ExportCsvFromCache {
            dsn,
            run_id,
            file_name,
            columns,
            values,
            row_count,
        } => {
            let tx = action_tx.clone();
            let exporter = Arc::clone(cached_result_exporter);

            query_tasks
                .replace(async move {
                    let result = exporter
                        .export_cached_result_to_csv(file_name, columns, values)
                        .await;

                    match result {
                        Ok(path) => {
                            tx.send(Action::CsvExportSucceeded {
                                dsn,
                                run_id,
                                path: path.display().to_string(),
                                row_count,
                            })
                            .await
                            .ok();
                        }
                        Err(error) => {
                            tx.send(Action::CsvExportFailed { dsn, run_id, error })
                                .await
                                .ok();
                        }
                    }
                })
                .await;
        }

        _ => unreachable!("query::run called with non-query effect"),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use crate::cmd::cache::TtlCache;
    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures;
    use crate::domain::{DatabaseDiagnostic, DiagnosticLevel, WriteExecutionResult};
    use crate::model::app_state::AppState;
    use crate::ports::outbound::AccessMode;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::update::action::Action;

    async fn run_effect(effect: Effect, executor: MockQueryExecutor) -> Action {
        let cache = TtlCache::new(300);
        let (tx, mut rx) = mpsc::channel(8);
        let runner = test_fixtures::make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(executor),
            Arc::new(MockConnectionStore::new()),
            cache,
            tx,
        );
        let run = test_fixtures::run_one_effect(
            &runner,
            effect,
            AppState::new("test".to_string()),
            RefCell::new(CompletionEngine::new()),
            &mut rx,
            Some(Duration::from_millis(500)),
        )
        .await
        .unwrap();

        run.actions.into_iter().next().expect("action dispatched")
    }

    mod query_history_append {
        use tokio::sync::{Barrier, Mutex, oneshot};

        use super::*;
        use crate::cmd::single_task_owner::SingleTaskOwner;
        use crate::domain::connection::ConnectionId;
        use crate::domain::query_history::{
            QueryHistoryEntry, QueryHistoryScope, QueryResultStatus,
        };
        use crate::domain::{CommandTag, DatabaseType};
        use crate::ports::outbound::{
            CachedResultExporter, DbOperationError, QueryExecutor, QueryHistoryError,
            QueryHistoryStore,
        };

        #[derive(Clone)]
        struct HistoryCall {
            project_name: String,
            scope: QueryHistoryScope,
            entry: QueryHistoryEntry,
        }

        #[derive(Clone)]
        struct RecordingQueryHistoryStore {
            calls: Arc<Mutex<Vec<HistoryCall>>>,
            append_barrier: Arc<Barrier>,
            append_started: Arc<Mutex<Option<oneshot::Sender<()>>>>,
            append_finished: Arc<Mutex<Option<oneshot::Sender<()>>>>,
        }

        #[async_trait::async_trait]
        impl QueryHistoryStore for RecordingQueryHistoryStore {
            async fn append(
                &self,
                project_name: &str,
                scope: &QueryHistoryScope,
                entry: &QueryHistoryEntry,
            ) -> Result<(), QueryHistoryError> {
                let signal = self.append_started.lock().await.take();
                if let Some(signal) = signal {
                    signal.send(()).ok();
                }
                self.calls.lock().await.push(HistoryCall {
                    project_name: project_name.to_string(),
                    scope: scope.clone(),
                    entry: entry.clone(),
                });
                self.append_barrier.wait().await;
                let signal = self.append_finished.lock().await.take();
                if let Some(signal) = signal {
                    signal.send(()).ok();
                }
                Ok(())
            }

            async fn load(
                &self,
                _project_name: &str,
                _scope: &QueryHistoryScope,
            ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
                Ok(Vec::new())
            }
        }

        fn history_store() -> (
            RecordingQueryHistoryStore,
            oneshot::Receiver<()>,
            oneshot::Receiver<()>,
        ) {
            let (started_tx, started_rx) = oneshot::channel();
            let (finished_tx, finished_rx) = oneshot::channel();
            (
                RecordingQueryHistoryStore {
                    calls: Arc::new(Mutex::new(Vec::new())),
                    append_barrier: Arc::new(Barrier::new(2)),
                    append_started: Arc::new(Mutex::new(Some(started_tx))),
                    append_finished: Arc::new(Mutex::new(Some(finished_tx))),
                },
                started_rx,
                finished_rx,
            )
        }

        fn connected_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_target(
                &ConnectionId::from_string("connection"),
                "connection",
                DatabaseType::MySQL,
                "dsn://test",
                Some("analytics"),
            );
            state
        }

        async fn run_with_history(
            effect: Effect,
            executor: MockQueryExecutor,
            state: AppState,
            store: RecordingQueryHistoryStore,
            append_started_rx: oneshot::Receiver<()>,
            append_finished_rx: oneshot::Receiver<()>,
            expect_append: bool,
        ) -> (Action, Vec<HistoryCall>) {
            let (tx, mut rx) = mpsc::channel(8);
            let query_executor: Arc<dyn QueryExecutor> = Arc::new(executor);
            let query_history_store: Arc<dyn QueryHistoryStore> = Arc::new(store.clone());
            let cached_result_exporter: Arc<dyn CachedResultExporter> =
                Arc::new(test_fixtures::TestCachedResultExporter);
            let query_tasks = SingleTaskOwner::default();

            super::super::run(
                effect,
                &tx,
                &query_executor,
                &query_history_store,
                &cached_result_exporter,
                &query_tasks,
                &state,
            )
            .await;

            let action =
                test_fixtures::recv_action_with_timeout(&mut rx, Duration::from_secs(1)).await;
            if expect_append {
                drop(append_started_rx);
                let append_release = Arc::clone(&store.append_barrier);
                tokio::time::timeout(Duration::from_secs(1), append_release.wait())
                    .await
                    .expect("history append should reach its release point");
                tokio::time::timeout(Duration::from_secs(1), append_finished_rx)
                    .await
                    .expect("history append should finish")
                    .expect("history append signal should be sent");
            } else {
                assert!(
                    tokio::time::timeout(Duration::from_millis(1), append_started_rx)
                        .await
                        .is_err(),
                    "history append should not be spawned"
                );
            }

            let calls = store.calls.lock().await.clone();
            (action, calls)
        }

        fn assert_history_call(
            calls: &[HistoryCall],
            query: &str,
            result_status: QueryResultStatus,
            affected_rows: Option<u64>,
        ) {
            assert_eq!(calls.len(), 1);
            let call = &calls[0];
            assert_eq!(call.project_name, "test");
            assert_eq!(call.scope.connection_id.as_str(), "connection");
            assert_eq!(call.scope.database.as_deref(), Some("analytics"));
            assert_eq!(call.entry.query, query);
            assert_eq!(call.entry.connection_id.as_str(), "connection");
            assert_eq!(call.entry.database.as_deref(), Some("analytics"));
            assert_eq!(call.entry.result_status, result_status);
            assert_eq!(call.entry.affected_rows, affected_rows);
        }

        #[tokio::test(start_paused = true)]
        async fn adhoc_success_appends_query_history_after_execution() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Ok(test_fixtures::sample_query_result().with_command_tag(CommandTag::Update(7)))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 1,
                    query: "UPDATE users SET active = true".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
                connected_state(),
                store,
                append_started_rx,
                append_rx,
                true,
            )
            .await;

            assert!(matches!(action, Action::QueryCompleted { run_id: 1, .. }));
            assert_history_call(
                &calls,
                "UPDATE users SET active = true",
                QueryResultStatus::Success,
                Some(7),
            );
        }

        #[tokio::test(start_paused = true)]
        async fn adhoc_failure_appends_failed_query_history() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Err(DbOperationError::QueryFailed("syntax error".to_string()))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 2,
                    query: "SELECT broken".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
                connected_state(),
                store,
                append_started_rx,
                append_rx,
                true,
            )
            .await;

            assert!(matches!(action, Action::QueryFailed { run_id: 2, .. }));
            assert_history_call(&calls, "SELECT broken", QueryResultStatus::Failed, None);
        }

        #[tokio::test(start_paused = true)]
        async fn write_success_appends_affected_rows_to_query_history() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_write().once().returning(|_, _, _| {
                Ok(WriteExecutionResult {
                    affected_rows: 7,
                    diagnostics: Vec::new(),
                })
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 3,
                    query: "UPDATE users SET active = true".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
                connected_state(),
                store,
                append_started_rx,
                append_rx,
                true,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExecuteWriteSucceeded {
                    run_id: 3,
                    affected_rows: 7,
                    ..
                }
            ));
            assert_history_call(
                &calls,
                "UPDATE users SET active = true",
                QueryResultStatus::Success,
                Some(7),
            );
        }

        #[tokio::test(start_paused = true)]
        async fn write_failure_appends_failed_query_history() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_write().once().returning(|_, _, _| {
                Err(DbOperationError::QueryFailed("write failed".to_string()))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 4,
                    query: "UPDATE users SET active = false".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
                connected_state(),
                store,
                append_started_rx,
                append_rx,
                true,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExecuteWriteFailed { run_id: 4, .. }
            ));
            assert_history_call(
                &calls,
                "UPDATE users SET active = false",
                QueryResultStatus::Failed,
                None,
            );
        }

        #[tokio::test(start_paused = true)]
        async fn history_disabled_does_not_spawn_append() {
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_adhoc()
                .once()
                .returning(|_, _, _| Ok(test_fixtures::sample_query_result()));
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 5,
                    query: "SELECT 1".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
                AppState::new("test".to_string()),
                store,
                append_started_rx,
                append_rx,
                false,
            )
            .await;

            assert!(matches!(action, Action::QueryCompleted { run_id: 5, .. }));
            assert!(calls.is_empty());
        }

        #[tokio::test(start_paused = true)]
        async fn adhoc_failure_history_disabled_does_not_spawn_append() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Err(DbOperationError::QueryFailed("syntax error".to_string()))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 8,
                    query: "SELECT broken".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
                AppState::new("test".to_string()),
                store,
                append_started_rx,
                append_rx,
                false,
            )
            .await;

            assert!(matches!(action, Action::QueryFailed { run_id: 8, .. }));
            assert!(calls.is_empty());
        }

        #[tokio::test(start_paused = true)]
        async fn write_success_history_disabled_does_not_spawn_append() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_write().once().returning(|_, _, _| {
                Ok(WriteExecutionResult {
                    affected_rows: 7,
                    diagnostics: Vec::new(),
                })
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 6,
                    query: "UPDATE users SET active = true".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
                AppState::new("test".to_string()),
                store,
                append_started_rx,
                append_rx,
                false,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExecuteWriteSucceeded {
                    run_id: 6,
                    affected_rows: 7,
                    ..
                }
            ));
            assert!(calls.is_empty());
        }

        #[tokio::test(start_paused = true)]
        async fn write_failure_history_disabled_does_not_spawn_append() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_write().once().returning(|_, _, _| {
                Err(DbOperationError::QueryFailed("write failed".to_string()))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 7,
                    query: "UPDATE users SET active = false".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
                AppState::new("test".to_string()),
                store,
                append_started_rx,
                append_rx,
                false,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExecuteWriteFailed { run_id: 7, .. }
            ));
            assert!(calls.is_empty());
        }
    }

    mod explain_plan_text {
        use crate::domain::{
            QueryResult, QuerySource, QueryValue, SqliteExplainPlanError,
            postgres_explain_plan_text_from_result, sqlite_explain_query_plan_text_from_result,
        };

        #[test]
        fn sqlite_query_plan_uses_detail_column() {
            let result = QueryResult::success(
                "EXPLAIN QUERY PLAN SELECT * FROM users".to_string(),
                vec![
                    "id".to_string(),
                    "parent".to_string(),
                    "notused".to_string(),
                    "detail".to_string(),
                ],
                vec![
                    vec![
                        "2".to_string(),
                        "0".to_string(),
                        "56".to_string(),
                        "SEARCH users USING INDEX idx_users_name".to_string(),
                    ],
                    vec![
                        "5".to_string(),
                        "2".to_string(),
                        "0".to_string(),
                        "SCAN orders".to_string(),
                    ],
                ],
                1,
                QuerySource::Adhoc,
            );

            assert_eq!(
                sqlite_explain_query_plan_text_from_result(&result).unwrap(),
                "SEARCH users USING INDEX idx_users_name\n  - SCAN orders"
            );
        }

        #[test]
        fn sqlite_query_plan_requires_structured_columns() {
            for &missing_column in &["id", "parent", "detail"] {
                let columns = ["id", "parent", "notused", "detail"]
                    .into_iter()
                    .filter(|column| *column != missing_column)
                    .map(str::to_string)
                    .collect();
                let result = QueryResult::success(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    columns,
                    vec![],
                    1,
                    QuerySource::Adhoc,
                );

                assert!(matches!(
                    sqlite_explain_query_plan_text_from_result(&result),
                    Err(SqliteExplainPlanError::MissingColumn(column))
                        if column == missing_column
                ));
            }
        }

        #[test]
        fn sqlite_query_plan_rejects_malformed_structured_values() {
            for (column, row) in [
                ("id", vec!["not-an-id", "0", "0", "SCAN users"]),
                ("parent", vec!["2", "not-a-parent", "0", "SCAN users"]),
            ] {
                let result = QueryResult::success(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    vec![
                        "id".to_string(),
                        "parent".to_string(),
                        "notused".to_string(),
                        "detail".to_string(),
                    ],
                    vec![row.into_iter().map(str::to_string).collect()],
                    1,
                    QuerySource::Adhoc,
                );

                assert!(matches!(
                    sqlite_explain_query_plan_text_from_result(&result),
                    Err(SqliteExplainPlanError::InvalidValue {
                        row: 0,
                        column: invalid_column,
                        ..
                    }) if invalid_column == column
                ));
            }
        }

        #[test]
        fn sqlite_query_plan_rejects_missing_row_values() {
            for (missing_column, row) in [
                ("id", vec![]),
                ("parent", vec!["2"]),
                ("detail", vec!["2", "0", "0"]),
            ] {
                let result = QueryResult::success(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    vec![
                        "id".to_string(),
                        "parent".to_string(),
                        "notused".to_string(),
                        "detail".to_string(),
                    ],
                    vec![row.into_iter().map(str::to_string).collect()],
                    1,
                    QuerySource::Adhoc,
                );

                assert!(matches!(
                    sqlite_explain_query_plan_text_from_result(&result),
                    Err(SqliteExplainPlanError::MissingValue {
                        row: 0,
                        column,
                    }) if column == missing_column
                ));
            }
        }

        #[test]
        fn sqlite_query_plan_rejects_non_text_structured_values() {
            let result = QueryResult::success_with_values(
                "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                vec![
                    "id".to_string(),
                    "parent".to_string(),
                    "notused".to_string(),
                    "detail".to_string(),
                ],
                vec![vec![
                    QueryValue::text("2"),
                    QueryValue::text("0"),
                    QueryValue::text("0"),
                    QueryValue::Null,
                ]],
                1,
                QuerySource::Adhoc,
            );

            assert!(matches!(
                sqlite_explain_query_plan_text_from_result(&result),
                Err(SqliteExplainPlanError::InvalidValue {
                    row: 0,
                    column: "detail",
                    value,
                }) if value == "NULL"
            ));
        }

        #[test]
        fn postgres_plan_keeps_first_column_fallback() {
            let result = QueryResult::success(
                "EXPLAIN SELECT * FROM users".to_string(),
                vec!["QUERY PLAN".to_string()],
                vec![vec!["Seq Scan on users".to_string()]],
                1,
                QuerySource::Adhoc,
            );

            assert_eq!(
                postgres_explain_plan_text_from_result(&result),
                "Seq Scan on users"
            );
        }
    }
    mod cached_csv_export_effect {
        use std::cell::RefCell;
        use std::sync::Arc;
        use std::time::Duration;

        use tokio::sync::mpsc;

        use crate::cmd::cache::TtlCache;
        use crate::cmd::completion_engine::CompletionEngine;
        use crate::cmd::effect::Effect;
        use crate::cmd::test_fixtures;
        use crate::domain::QueryValue;
        use crate::model::app_state::AppState;
        use crate::ports::outbound::connection_store::MockConnectionStore;
        use crate::ports::outbound::metadata::MockMetadataProvider;
        use crate::ports::outbound::query_executor::MockQueryExecutor;
        use crate::ports::outbound::{CachedResultExporter, DbOperationError};
        use crate::update::action::Action;

        fn test_file_name(label: &str) -> String {
            format!("cached_{label}_{}", std::process::id())
        }

        struct FailingCachedResultExporter;

        #[async_trait::async_trait]
        impl CachedResultExporter for FailingCachedResultExporter {
            async fn export_cached_result_to_csv(
                &self,
                _file_name: String,
                _columns: Vec<String>,
                _values: Vec<Vec<QueryValue>>,
            ) -> Result<std::path::PathBuf, DbOperationError> {
                Err(DbOperationError::QueryFailed("export failed".to_string()))
            }
        }

        #[tokio::test]
        async fn dispatches_success() {
            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExportCsvFromCache {
                    dsn: "sqlite:///tmp/test.db".to_string(),
                    run_id: 7,
                    file_name: test_file_name("success"),
                    columns: vec!["id".to_string(), "payload".to_string()],
                    values: vec![vec![
                        QueryValue::SqlLiteral("1".to_string()),
                        QueryValue::Blob(vec![0xAB, 0xCD]),
                    ]],
                    row_count: Some(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            let Action::CsvExportSucceeded {
                path, row_count, ..
            } = action
            else {
                panic!("expected CSV export success action");
            };

            assert_eq!(row_count, Some(1));
            assert!(path.contains("cached_success"));
        }

        #[tokio::test]
        async fn dispatches_failure_when_exporter_fails() {
            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_cached_result_exporter(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
                Arc::new(FailingCachedResultExporter),
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExportCsvFromCache {
                    dsn: "sqlite:///tmp/test.db".to_string(),
                    run_id: 8,
                    file_name: test_file_name("failure"),
                    columns: vec!["id".to_string()],
                    values: vec![vec![QueryValue::SqlLiteral("1".to_string())]],
                    row_count: Some(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(action, Action::CsvExportFailed { run_id: 8, .. }));
        }
    }

    mod execute_preview {
        use std::cell::RefCell;
        use std::sync::Arc;
        use std::time::Duration;

        use tokio::sync::mpsc;

        use crate::cmd::cache::TtlCache;
        use crate::cmd::completion_engine::CompletionEngine;
        use crate::cmd::effect::Effect;
        use crate::cmd::test_fixtures;

        use crate::model::app_state::AppState;
        use crate::ports::outbound::DbOperationError;
        use crate::ports::outbound::connection_store::MockConnectionStore;
        use crate::ports::outbound::metadata::MockMetadataProvider;
        use crate::ports::outbound::query_executor::MockQueryExecutor;
        use crate::update::action::{Action, QueryFailureContext};

        #[tokio::test]
        async fn success_returns_query_completed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| Ok(test_fixtures::sample_query_result()));

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExecutePreview {
                    dsn: "dsn://test".to_string(),
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                    run_id: 8,
                    limit: 100,
                    offset: 0,
                    target_page: 0,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::QueryCompleted { .. }),
                "expected QueryCompleted, got {action:?}"
            );
        }

        #[tokio::test]
        async fn error_returns_query_failed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| {
                    Err(DbOperationError::QueryFailed("syntax error".to_string()))
                });

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExecutePreview {
                    dsn: "dsn://test".to_string(),
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                    run_id: 8,
                    limit: 100,
                    offset: 0,
                    target_page: 0,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::QueryFailed {
                        context: QueryFailureContext::Preview { .. },
                        ..
                    }
                ),
                "expected QueryFailed, got {action:?}"
            );
        }
    }

    mod diagnostic_masking {
        use super::*;
        use crate::cmd::browse::query::mask_mysql_diagnostics;

        #[test]
        fn masks_credentials_in_success_diagnostic_messages() {
            let cases = [
                (
                    "mysql://user:dsn-secret@host",
                    "mysql://user:****@host",
                    DiagnosticLevel::Warning,
                    1001,
                ),
                (
                    "password=kv-secret",
                    "password=****",
                    DiagnosticLevel::Note,
                    1002,
                ),
                (
                    "MYSQL_PWD=environment-secret",
                    "MYSQL_PWD=****",
                    DiagnosticLevel::Warning,
                    1003,
                ),
                (
                    "Data truncated",
                    "Data truncated",
                    DiagnosticLevel::Note,
                    1004,
                ),
            ];

            for (message, expected, level, code) in cases {
                let mut diagnostics = vec![DatabaseDiagnostic {
                    level,
                    code,
                    message: message.to_string(),
                }];

                mask_mysql_diagnostics(&mut diagnostics);

                assert_eq!(diagnostics[0].message, expected);
                assert_eq!(diagnostics[0].level, level);
                assert_eq!(diagnostics[0].code, code);
            }
        }

        #[tokio::test]
        async fn execute_adhoc_masks_credentials_before_action() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Ok(
                    test_fixtures::sample_query_result().with_mysql_diagnostics(vec![
                        DatabaseDiagnostic {
                            level: DiagnosticLevel::Warning,
                            code: 1001,
                            message: "mysql://user:secret@host".to_string(),
                        },
                    ]),
                )
            });

            let action = run_effect(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 1,
                    query: "SELECT 1".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            match action {
                Action::QueryCompleted { result, .. } => assert_eq!(
                    result.mysql_diagnostics,
                    vec![DatabaseDiagnostic {
                        level: DiagnosticLevel::Warning,
                        code: 1001,
                        message: "mysql://user:****@host".to_string(),
                    }]
                ),
                action => panic!("unexpected action: {action:?}"),
            }
        }

        #[tokio::test]
        async fn execute_write_masks_credentials_before_action() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_write().once().returning(|_, _, _| {
                Ok(WriteExecutionResult {
                    affected_rows: 1,
                    diagnostics: vec![DatabaseDiagnostic {
                        level: DiagnosticLevel::Warning,
                        code: 1265,
                        message: "Data truncated password=secret".to_string(),
                    }],
                })
            });

            let action = run_effect(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 3,
                    query: "INSERT INTO users VALUES (1)".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
            )
            .await;

            match action {
                Action::ExecuteWriteSucceeded { diagnostics, .. } => assert_eq!(
                    diagnostics,
                    vec![DatabaseDiagnostic {
                        level: DiagnosticLevel::Warning,
                        code: 1265,
                        message: "Data truncated password=****".to_string(),
                    }]
                ),
                action => panic!("unexpected action: {action:?}"),
            }
        }
    }

    mod execute_access_mode {
        use super::*;
        use crate::domain::{DatabaseType, QueryResult, QuerySource, QueryValue};
        use crate::ports::outbound::DbOperationError;

        #[tokio::test]
        async fn execute_adhoc_forwards_access_mode() {
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_adhoc()
                .once()
                .withf(|_, _, access_mode| *access_mode == AccessMode::ReadOnly)
                .returning(|_, _, _| Ok(test_fixtures::sample_query_result()));

            let action = run_effect(
                Effect::ExecuteAdhoc {
                    dsn: "dsn://test".to_string(),
                    run_id: 1,
                    query: "SELECT 1".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            assert!(matches!(action, Action::QueryCompleted { run_id: 1, .. }));
        }

        #[tokio::test]
        async fn execute_explain_forwards_access_mode() {
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_adhoc()
                .once()
                .withf(|_, _, access_mode| *access_mode == AccessMode::ReadOnly)
                .returning(|_, _, _| Ok(test_fixtures::sample_query_result()));

            let action = run_effect(
                Effect::ExecuteExplain {
                    dsn: "dsn://test".to_string(),
                    database_type: DatabaseType::PostgreSQL,
                    database_generation: 0,
                    run_id: 2,
                    query: "EXPLAIN SELECT 1".to_string(),
                    source_query: "SELECT 1".to_string(),
                    is_analyze: false,
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            assert!(matches!(action, Action::ExplainCompleted { run_id: 2, .. }));
        }

        #[tokio::test]
        async fn sqlite_explain_parse_failure_returns_explain_failed() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Ok(QueryResult::success(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    vec!["id".to_string(), "parent".to_string()],
                    vec![vec!["2".to_string(), "0".to_string()]],
                    1,
                    QuerySource::Adhoc,
                ))
            });

            let action = run_effect(
                Effect::ExecuteExplain {
                    dsn: "dsn://test".to_string(),
                    database_type: DatabaseType::SQLite,
                    database_generation: 0,
                    run_id: 2,
                    query: "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    source_query: "SELECT 1".to_string(),
                    is_analyze: false,
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            let Action::ExplainFailed { error, run_id, .. } = action else {
                panic!("expected ExplainFailed action");
            };
            assert_eq!(run_id, 2);
            assert!(matches!(
                error,
                DbOperationError::QueryFailed(details)
                    if details.contains("missing required column: detail")
            ));
        }

        #[tokio::test]
        async fn sqlite_explain_success_returns_explain_completed() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Ok(QueryResult::success(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    vec![
                        "id".to_string(),
                        "parent".to_string(),
                        "notused".to_string(),
                        "detail".to_string(),
                    ],
                    vec![vec![
                        "2".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "SCAN users".to_string(),
                    ]],
                    1,
                    QuerySource::Adhoc,
                ))
            });

            let action = run_effect(
                Effect::ExecuteExplain {
                    dsn: "dsn://test".to_string(),
                    database_type: DatabaseType::SQLite,
                    database_generation: 0,
                    run_id: 3,
                    query: "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    source_query: "SELECT 1".to_string(),
                    is_analyze: false,
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExplainCompleted {
                    run_id: 3,
                    plan_text,
                    ..
                } if plan_text == "SCAN users"
            ));
        }

        #[tokio::test]
        async fn sqlite_explain_non_text_detail_returns_explain_failed() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Ok(QueryResult::success_with_values(
                    "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    vec![
                        "id".to_string(),
                        "parent".to_string(),
                        "notused".to_string(),
                        "detail".to_string(),
                    ],
                    vec![vec![
                        QueryValue::text("2"),
                        QueryValue::text("0"),
                        QueryValue::text("0"),
                        QueryValue::Null,
                    ]],
                    1,
                    QuerySource::Adhoc,
                ))
            });

            let action = run_effect(
                Effect::ExecuteExplain {
                    dsn: "dsn://test".to_string(),
                    database_type: DatabaseType::SQLite,
                    database_generation: 0,
                    run_id: 4,
                    query: "EXPLAIN QUERY PLAN SELECT 1".to_string(),
                    source_query: "SELECT 1".to_string(),
                    is_analyze: false,
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            assert!(matches!(
                action,
                Action::ExplainFailed {
                    run_id: 4,
                    error: DbOperationError::QueryFailed(details),
                    ..
                } if details.contains("invalid detail value: NULL")
            ));
        }

        #[tokio::test]
        async fn execute_write_forwards_access_mode_and_diagnostics() {
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_write()
                .once()
                .withf(|_, _, access_mode| *access_mode == AccessMode::ReadWrite)
                .returning(|_, _, _| {
                    Ok(WriteExecutionResult {
                        affected_rows: 1,
                        diagnostics: vec![DatabaseDiagnostic {
                            level: DiagnosticLevel::Warning,
                            code: 1265,
                            message: "Data truncated".to_string(),
                        }],
                    })
                });

            let action = run_effect(
                Effect::ExecuteWrite {
                    dsn: "dsn://test".to_string(),
                    run_id: 3,
                    query: "INSERT INTO users VALUES (1)".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
            )
            .await;

            match action {
                Action::ExecuteWriteSucceeded {
                    run_id: 3,
                    affected_rows: 1,
                    diagnostics,
                    ..
                } => assert_eq!(
                    diagnostics,
                    vec![DatabaseDiagnostic {
                        level: DiagnosticLevel::Warning,
                        code: 1265,
                        message: "Data truncated".to_string(),
                    }]
                ),
                action => panic!("unexpected action: {action:?}"),
            }
        }
    }
}
