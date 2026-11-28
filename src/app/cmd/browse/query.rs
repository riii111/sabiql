use std::sync::Arc;
use std::time::SystemTime;

use crate::domain::command_tag::CommandTag;
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope, QueryResultStatus};
use crate::domain::{DatabaseDiagnostic, DatabaseType, QueryResult};
use crate::domain::{
    mysql_explain_plan_text_from_result, postgres_explain_plan_text_from_result,
    sqlite_explain_query_plan_text_from_result,
};
use crate::model::app_state::AppState;
use crate::policy::mask_password;
use crate::ports::outbound::{
    AccessMode, CachedResultExporter, DbOperationError, QueryExecutor, QueryHistoryStore,
};
use crate::update::action::{Action, CsvExport, ExplainPlan, QueryContext, WriteOutcome};

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

pub(in crate::cmd) struct QueryHistoryTarget {
    store: Arc<dyn QueryHistoryStore>,
    project_name: String,
    scope: QueryHistoryScope,
}

impl QueryHistoryTarget {
    pub(in crate::cmd) fn for_state(
        store: &Arc<dyn QueryHistoryStore>,
        state: &AppState,
    ) -> Option<Self> {
        Some(Self {
            store: Arc::clone(store),
            project_name: state.project_name().to_string(),
            scope: state.session.query_history_scope()?,
        })
    }

    fn spawn_append(self, query: String, result_status: QueryResultStatus, rows: Option<u64>) {
        let entry = QueryHistoryEntry::new_with_database(
            query,
            utc_now_iso8601(),
            self.scope.connection_id.clone(),
            self.scope.database.clone(),
            result_status,
            rows,
        );
        tokio::spawn(async move {
            let _ = self
                .store
                .append(&self.project_name, &self.scope, &entry)
                .await;
        });
    }
}

fn record_history<T>(
    history: Option<QueryHistoryTarget>,
    query: String,
    outcome: &Result<T, DbOperationError>,
    affected_rows: impl FnOnce(&T) -> Option<u64>,
) {
    let Some(history) = history else {
        return;
    };
    match outcome {
        Ok(value) => {
            history.spawn_append(query, QueryResultStatus::Success, affected_rows(value));
        }
        Err(_) => history.spawn_append(query, QueryResultStatus::Failed, None),
    }
}

pub(in crate::cmd) async fn execute_preview(
    executor: Arc<dyn QueryExecutor>,
    endpoint: Arc<ConnectionEndpoint>,
    schema: String,
    table: String,
    run_id: u64,
    context: QueryContext,
    limit: usize,
    offset: usize,
) -> Action {
    let outcome = executor
        .execute_preview(&endpoint, &schema, &table, limit, offset)
        .await
        .map(Arc::new);
    Action::QueryCompleted {
        run_id,
        context,
        outcome,
    }
}

pub(in crate::cmd) async fn execute_explain(
    executor: Arc<dyn QueryExecutor>,
    endpoint: Arc<ConnectionEndpoint>,
    database_generation: u64,
    run_id: u64,
    query: String,
    source_query: String,
    is_analyze: bool,
    access_mode: AccessMode,
) -> Action {
    let database_type = endpoint.engine();
    let outcome = executor
        .execute_adhoc(&endpoint, &query, access_mode)
        .await
        .and_then(|result| {
            let plan_text = match database_type {
                DatabaseType::SQLite => sqlite_explain_query_plan_text_from_result(&result)
                    .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?,
                DatabaseType::PostgreSQL => postgres_explain_plan_text_from_result(&result),
                DatabaseType::MySQL => mysql_explain_plan_text_from_result(&result),
            };
            Ok(ExplainPlan {
                database_type,
                query: source_query,
                plan_text,
                execution_time_ms: result.execution_time_ms,
            })
        });
    Action::ExplainCompleted {
        database_generation,
        run_id,
        is_analyze,
        outcome,
    }
}

pub(in crate::cmd) async fn execute_adhoc(
    executor: Arc<dyn QueryExecutor>,
    history: Option<QueryHistoryTarget>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    query: String,
    access_mode: AccessMode,
) -> Action {
    let outcome = executor
        .execute_adhoc(&endpoint, &query, access_mode)
        .await
        .map(|mut result| {
            mask_mysql_diagnostics(&mut result.mysql_diagnostics);
            result
        });
    record_history(history, query, &outcome, |result| {
        result
            .command_tag
            .as_ref()
            .and_then(CommandTag::affected_rows)
    });
    Action::QueryCompleted {
        run_id,
        context: QueryContext::Adhoc,
        outcome: outcome.map(Arc::new),
    }
}

pub(in crate::cmd) async fn execute_write(
    executor: Arc<dyn QueryExecutor>,
    history: Option<QueryHistoryTarget>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    query: String,
    access_mode: AccessMode,
) -> Action {
    let outcome = executor
        .execute_write(&endpoint, &query, access_mode)
        .await
        .map(|mut result| {
            mask_mysql_diagnostics(&mut result.diagnostics);
            WriteOutcome {
                affected_rows: result.affected_rows,
                diagnostics: result.diagnostics,
            }
        });
    record_history(history, query, &outcome, |result| {
        Some(result.affected_rows as u64)
    });
    Action::WriteExecuted { run_id, outcome }
}

pub(in crate::cmd) async fn export_csv(
    exporter: Arc<dyn CachedResultExporter>,
    run_id: u64,
    file_name: String,
    result: Arc<QueryResult>,
) -> Action {
    let row_count = result.data_row_count();
    let outcome = exporter
        .export_cached_result_to_csv(file_name, result)
        .await
        .map(|path| CsvExport {
            path: path.display().to_string(),
            row_count,
        });
    Action::CsvExported { run_id, outcome }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures;
    use crate::domain::{DatabaseDiagnostic, DiagnosticLevel, WriteExecutionResult};
    use crate::model::app_state::AppState;
    use crate::ports::outbound::AccessMode;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::test_support::endpoint;
    use crate::update::action::Action;

    async fn run_effect(effect: Effect, executor: MockQueryExecutor) -> Action {
        let (tx, mut rx) = mpsc::channel(8);
        let runner = test_fixtures::make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(executor),
            Arc::new(MockConnectionStore::new()),
            tx,
        );
        let run = test_fixtures::run_one_effect(
            &runner,
            effect,
            AppState::new("test".to_string()),
            &mut rx,
            Some(Duration::from_millis(500)),
        )
        .await
        .unwrap();

        run.actions.into_iter().next().expect("action dispatched")
    }

    mod query_history_append {
        use crate::update::action::WriteOutcome;
        use tokio::sync::{Barrier, Mutex, oneshot};

        use super::*;
        use crate::cmd::browse::query::{QueryHistoryTarget, execute_adhoc, execute_write};
        use crate::domain::CommandTag;
        use crate::domain::connection::ConnectionId;
        use crate::domain::query_history::{
            QueryHistoryEntry, QueryHistoryScope, QueryResultStatus,
        };
        use crate::ports::outbound::{
            DbOperationError, QueryExecutor, QueryHistoryError, QueryHistoryStore,
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
            state.session.activate_connection(
                &ConnectionId::from_string("connection"),
                "connection",
                &endpoint::mysql("dsn://test", Some("analytics")),
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
            let query_executor: Arc<dyn QueryExecutor> = Arc::new(executor);
            let query_history_store: Arc<dyn QueryHistoryStore> = Arc::new(store.clone());
            let history = QueryHistoryTarget::for_state(&query_history_store, &state);

            let action = match effect {
                Effect::ExecuteAdhoc {
                    endpoint,
                    run_id,
                    query,
                    access_mode,
                } => {
                    execute_adhoc(
                        query_executor,
                        history,
                        endpoint,
                        run_id,
                        query,
                        access_mode,
                    )
                    .await
                }
                Effect::ExecuteWrite {
                    endpoint,
                    run_id,
                    query,
                    access_mode,
                } => {
                    execute_write(
                        query_executor,
                        history,
                        endpoint,
                        run_id,
                        query,
                        access_mode,
                    )
                    .await
                }
                other => panic!("unexpected history effect: {other:?}"),
            };
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
                    endpoint: endpoint::mysql("dsn://test", Some("analytics")),
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

            assert!(matches!(
                action,
                Action::QueryCompleted {
                    run_id: 1,
                    outcome: Ok(_),
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
        async fn adhoc_failure_appends_failed_query_history() {
            let mut executor = MockQueryExecutor::new();
            executor.expect_execute_adhoc().once().returning(|_, _, _| {
                Err(DbOperationError::QueryFailed("syntax error".to_string()))
            });
            let (store, append_started_rx, append_rx) = history_store();

            let (action, calls) = run_with_history(
                Effect::ExecuteAdhoc {
                    endpoint: endpoint::mysql("dsn://test", Some("analytics")),
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

            assert!(matches!(
                action,
                Action::QueryCompleted {
                    run_id: 2,
                    outcome: Err(_),
                    ..
                }
            ));
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
                    endpoint: endpoint::mysql("dsn://test", Some("analytics")),
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
                Action::WriteExecuted {
                    run_id: 3,
                    outcome: Ok(WriteOutcome {
                        affected_rows: 7,
                        ..
                    }),
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
                    endpoint: endpoint::mysql("dsn://test", Some("analytics")),
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
                Action::WriteExecuted {
                    run_id: 4,
                    outcome: Err(_),
                    ..
                }
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
                    endpoint: endpoint::postgres("dsn://test"),
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

            assert!(matches!(
                action,
                Action::QueryCompleted {
                    run_id: 5,
                    outcome: Ok(_),
                    ..
                }
            ));
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
                    endpoint: endpoint::postgres("dsn://test"),
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

            assert!(matches!(
                action,
                Action::QueryCompleted {
                    run_id: 8,
                    outcome: Err(_),
                    ..
                }
            ));
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
                    endpoint: endpoint::postgres("dsn://test"),
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
                Action::WriteExecuted {
                    run_id: 6,
                    outcome: Ok(WriteOutcome {
                        affected_rows: 7,
                        ..
                    }),
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
                    endpoint: endpoint::postgres("dsn://test"),
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
                Action::WriteExecuted {
                    run_id: 7,
                    outcome: Err(_),
                    ..
                }
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
        use crate::update::action::CsvExport;
        use std::sync::Arc;
        use std::time::Duration;

        use tokio::sync::mpsc;

        use crate::cmd::effect::Effect;
        use crate::cmd::test_fixtures;
        use crate::domain::{QueryResult, QuerySource, QueryValue};
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
                _result: Arc<QueryResult>,
            ) -> Result<std::path::PathBuf, DbOperationError> {
                Err(DbOperationError::QueryFailed("export failed".to_string()))
            }
        }

        #[tokio::test]
        async fn dispatches_success() {
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExportCsvFromCache {
                    run_id: 7,
                    file_name: test_file_name("success"),
                    result: Arc::new(QueryResult::success_with_values(
                        "SELECT id, payload FROM files".to_string(),
                        vec!["id".to_string(), "payload".to_string()],
                        vec![vec![
                            QueryValue::SqlLiteral("1".to_string()),
                            QueryValue::Blob(vec![0xAB, 0xCD]),
                        ]],
                        1,
                        QuerySource::Adhoc,
                    )),
                },
                AppState::new("test".to_string()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            let Action::CsvExported {
                outcome: Ok(CsvExport { path, row_count }),
                ..
            } = action
            else {
                panic!("expected CSV export success action");
            };

            assert_eq!(row_count, 1);
            assert!(path.contains("cached_success"));
        }

        #[tokio::test]
        async fn dispatches_failure_when_exporter_fails() {
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_cached_result_exporter(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
                Arc::new(FailingCachedResultExporter),
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExportCsvFromCache {
                    run_id: 8,
                    file_name: test_file_name("failure"),
                    result: Arc::new(QueryResult::success_with_values(
                        "SELECT id FROM files".to_string(),
                        vec!["id".to_string()],
                        vec![vec![QueryValue::SqlLiteral("1".to_string())]],
                        1,
                        QuerySource::Adhoc,
                    )),
                },
                AppState::new("test".to_string()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::CsvExported {
                    run_id: 8,
                    outcome: Err(_),
                    ..
                }
            ));
        }
    }

    mod execute_preview {
        use std::sync::Arc;
        use std::time::Duration;

        use tokio::sync::mpsc;

        use crate::cmd::effect::Effect;
        use crate::cmd::test_fixtures;

        use crate::model::app_state::AppState;
        use crate::ports::outbound::DbOperationError;
        use crate::ports::outbound::connection_store::MockConnectionStore;
        use crate::ports::outbound::metadata::MockMetadataProvider;
        use crate::ports::outbound::query_executor::MockQueryExecutor;
        use crate::test_support::endpoint;
        use crate::update::action::{Action, QueryContext};

        #[tokio::test]
        async fn success_returns_query_completed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| Ok(test_fixtures::sample_query_result()));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExecutePreview {
                    endpoint: endpoint::postgres("dsn://test"),
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                    run_id: 8,
                    limit: 100,
                    offset: 0,
                    target_page: 0,
                },
                AppState::new("test".to_string()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::QueryCompleted { outcome: Ok(_), .. }),
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

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::ExecutePreview {
                    endpoint: endpoint::postgres("dsn://test"),
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                    run_id: 8,
                    limit: 100,
                    offset: 0,
                    target_page: 0,
                },
                AppState::new("test".to_string()),
                &mut rx,
                Some(Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::QueryCompleted {
                        context: QueryContext::Preview { .. },
                        outcome: Err(_),
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
        use crate::update::action::WriteOutcome;

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
                    endpoint: endpoint::postgres("dsn://test"),
                    run_id: 1,
                    query: "SELECT 1".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            match action {
                Action::QueryCompleted {
                    outcome: Ok(result),
                    ..
                } => assert_eq!(
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
                    endpoint: endpoint::postgres("dsn://test"),
                    run_id: 3,
                    query: "INSERT INTO users VALUES (1)".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
            )
            .await;

            match action {
                Action::WriteExecuted {
                    outcome: Ok(WriteOutcome { diagnostics, .. }),
                    ..
                } => assert_eq!(
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
        use crate::domain::{QueryResult, QuerySource, QueryValue};
        use crate::ports::outbound::DbOperationError;
        use crate::update::action::ExplainPlan;
        use crate::update::action::WriteOutcome;

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
                    endpoint: endpoint::postgres("dsn://test"),
                    run_id: 1,
                    query: "SELECT 1".to_string(),
                    access_mode: AccessMode::ReadOnly,
                },
                executor,
            )
            .await;

            assert!(matches!(
                action,
                Action::QueryCompleted {
                    run_id: 1,
                    outcome: Ok(_),
                    ..
                }
            ));
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
                    endpoint: endpoint::postgres("dsn://test"),
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

            assert!(matches!(
                action,
                Action::ExplainCompleted {
                    run_id: 2,
                    outcome: Ok(_),
                    ..
                }
            ));
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
                    endpoint: endpoint::sqlite("dsn://test"),
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

            let Action::ExplainCompleted {
                run_id,
                outcome: Err(error),
                ..
            } = action
            else {
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
                    endpoint: endpoint::sqlite("dsn://test"),
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
                Action::ExplainCompleted { run_id: 3, outcome: Ok(ExplainPlan { plan_text, .. }), .. } if plan_text == "SCAN users"
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
                    endpoint: endpoint::sqlite("dsn://test"),
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
                Action::ExplainCompleted { run_id: 4, outcome: Err(DbOperationError::QueryFailed(details)), .. } if details.contains("invalid detail value: NULL")
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
                    endpoint: endpoint::postgres("dsn://test"),
                    run_id: 3,
                    query: "INSERT INTO users VALUES (1)".to_string(),
                    access_mode: AccessMode::ReadWrite,
                },
                executor,
            )
            .await;

            match action {
                Action::WriteExecuted {
                    run_id: 3,
                    outcome:
                        Ok(WriteOutcome {
                            affected_rows: 1,
                            diagnostics,
                        }),
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
