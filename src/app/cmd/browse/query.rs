use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::cmd::query_task::QueryTaskRegistry;
use crate::domain::DatabaseType;
use crate::domain::command_tag::CommandTag;
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope, QueryResultStatus};
use crate::domain::{
    mysql_explain_plan_text_from_result, postgres_explain_plan_text_from_result,
    sqlite_explain_query_plan_text_from_result,
};
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    CachedResultExporter, DbOperationError, QueryExecutor, QueryHistoryStore,
};
use crate::update::action::{Action, QueryCompletionContext, QueryFailureContext};

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
    query_history_store: &Arc<dyn QueryHistoryStore>,
    project_name: &str,
    scope: &QueryHistoryScope,
    query: &str,
    result_status: QueryResultStatus,
    affected_rows: Option<u64>,
) {
    let store = Arc::clone(query_history_store);
    let entry = QueryHistoryEntry::new_with_database(
        query.to_string(),
        utc_now_iso8601(),
        scope.connection_id.clone(),
        scope.database.clone(),
        result_status,
        affected_rows,
    );
    let project = project_name.to_string();
    let scope = scope.clone();
    tokio::spawn(async move {
        let _ = store.append(&project, &scope, &entry).await;
    });
}

pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_executor: &Arc<dyn QueryExecutor>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
    cached_result_exporter: &Arc<dyn CachedResultExporter>,
    query_tasks: &QueryTaskRegistry,
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
                .spawn(async move {
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
                .spawn(async move {
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
            let query_for_history = query.clone();
            query_tasks
                .spawn(async move {
                    let result = executor.execute_adhoc(&dsn, &query, access_mode).await;
                    match result {
                        Ok(result) => {
                            if let Some(scope) = &history_scope {
                                let rows = result
                                    .command_tag
                                    .as_ref()
                                    .and_then(CommandTag::affected_rows);
                                spawn_query_history_append(
                                    &history_store,
                                    &project,
                                    scope,
                                    &query_for_history,
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
                            if let Some(scope) = &history_scope {
                                spawn_query_history_append(
                                    &history_store,
                                    &project,
                                    scope,
                                    &query_for_history,
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
            let query_for_history = query.clone();

            query_tasks
                .spawn(async move {
                    match executor.execute_write(&dsn, &query, access_mode).await {
                        Ok(result) => {
                            if let Some(scope) = &history_scope {
                                spawn_query_history_append(
                                    &history_store,
                                    &project,
                                    scope,
                                    &query_for_history,
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
                            if let Some(scope) = &history_scope {
                                spawn_query_history_append(
                                    &history_store,
                                    &project,
                                    scope,
                                    &query_for_history,
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

        Effect::CountRowsForExport {
            dsn,
            run_id,
            count_query,
            export_query,
            file_name,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            query_tasks
                .spawn(async move {
                    let row_count = executor.count_query_rows(&dsn, &count_query).await.ok();
                    tx.send(Action::CsvExportRowsCounted {
                        dsn,
                        run_id,
                        row_count,
                        export_query,
                        file_name,
                    })
                    .await
                    .ok();
                })
                .await;
        }

        Effect::ExportCsv {
            dsn,
            run_id,
            query,
            file_name,
            row_count,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let export_dsn = dsn.clone();

            query_tasks
                .spawn(async move {
                    let result = executor
                        .export_to_csv(&export_dsn, &query, &file_name)
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
                .spawn(async move {
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

    mod explain_plan_text {
        use crate::domain::{
            QueryResult, QuerySource, SqliteExplainPlanError,
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

    mod execute_access_mode {
        use super::*;
        use crate::domain::{DatabaseType, QueryResult, QuerySource};
        use crate::ports::outbound::DbOperationError;

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
