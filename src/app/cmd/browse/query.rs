use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::cmd::query_task::QueryTaskRegistry;
use crate::domain::ConnectionId;
use crate::domain::QuerySource;
use crate::domain::command_tag::CommandTag;
use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};
use crate::domain::sqlite_explain_query_plan_text_from_result;
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    CachedResultExporter, DbOperationError, QueryExecutor, QueryHistoryStore,
};
use crate::update::action::Action;

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

fn save_query_history(
    query_history_store: &Arc<dyn QueryHistoryStore>,
    action_tx: &mpsc::Sender<Action>,
    project_name: &str,
    connection_id: &ConnectionId,
    query: &str,
    result_status: QueryResultStatus,
    affected_rows: Option<u64>,
) {
    let store = Arc::clone(query_history_store);
    let tx = action_tx.clone();
    let entry = QueryHistoryEntry::new(
        query.to_string(),
        utc_now_iso8601(),
        connection_id.clone(),
        result_status,
        affected_rows,
    );
    let project = project_name.to_string();
    let conn_id = connection_id.clone();
    tokio::spawn(async move {
        if let Err(e) = store.append(&project, &conn_id, &entry).await {
            let _ = tx.send(Action::QueryHistoryAppendFailed(e)).await;
        }
    });
}

fn resolve_export_path(file_name: &str) -> PathBuf {
    let now_sys = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now_sys.as_secs();
    let millis = now_sys.subsec_millis();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (y, m, d) = epoch_days_to_ymd(days as i64);
    let timestamp = format!("{y:04}{m:02}{d:02}_{hours:02}{minutes:02}{seconds:02}_{millis:03}");
    let file_stem = format!("sabiql_export_{file_name}_{timestamp}.csv");
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(file_stem)
}

struct ExportTempFile {
    path: PathBuf,
    cleanup: bool,
}

impl ExportTempFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup: true,
        }
    }

    fn disarm(&mut self) {
        self.cleanup = false;
    }
}

impl Drop for ExportTempFile {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn temporary_export_path(final_path: &Path) -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.csv");
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    final_path.with_file_name(format!(
        ".{file_name}.{}.{}.part",
        std::process::id(),
        sequence
    ))
}

async fn export_to_path<F, Fut>(final_path: PathBuf, export: F) -> Result<usize, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<usize, DbOperationError>>,
{
    let temporary_path = temporary_export_path(&final_path);
    let mut cleanup = ExportTempFile::new(temporary_path.clone());
    let row_count = export(temporary_path.clone()).await?;
    tokio::fs::rename(&temporary_path, &final_path)
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    cleanup.disarm();
    Ok(row_count)
}

#[allow(
    clippy::unused_async,
    reason = "consistent async interface for effect runner dispatch"
)]
pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_executor: &Arc<dyn QueryExecutor>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
    cached_result_exporter: &Arc<dyn CachedResultExporter>,
    query_tasks: &QueryTaskRegistry,
    state: &AppState,
) -> Result<()> {
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

            query_tasks.spawn(async move {
                match executor
                    .execute_preview(&dsn, &schema, &table, limit, offset)
                    .await
                {
                    Ok(result) => {
                        tx.send(Action::QueryCompleted {
                            dsn,
                            run_id,
                            result: Arc::new(result),
                            generation,
                            target_page: Some(target_page),
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        tx.send(Action::QueryFailed {
                            dsn,
                            run_id,
                            error: e,
                            generation,
                            source: QuerySource::Preview,
                        })
                        .await
                        .ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ExecuteExplain {
            dsn,
            run_id,
            query,
            source_query,
            is_analyze,
            access_mode,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            query_tasks.spawn(async move {
                match executor.execute_adhoc(&dsn, &query, access_mode).await {
                    Ok(result) => {
                        let plan_text = sqlite_explain_query_plan_text_from_result(&result);
                        tx.send(Action::ExplainCompleted {
                            dsn,
                            run_id,
                            query: source_query,
                            plan_text,
                            is_analyze,
                            execution_time_ms: result.execution_time_ms,
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        tx.send(Action::ExplainFailed {
                            dsn,
                            run_id,
                            error: e,
                        })
                        .await
                        .ok();
                    }
                }
            });
            Ok(())
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
            let history_tx = action_tx.clone();
            let project = state.runtime.project_name().to_string();
            let conn_id = state.session.active_connection_id().cloned();
            let query_for_history = query.clone();

            query_tasks.spawn(async move {
                match executor.execute_adhoc(&dsn, &query, access_mode).await {
                    Ok(result) => {
                        if let Some(cid) = &conn_id {
                            let rows = result
                                .command_tag
                                .as_ref()
                                .and_then(CommandTag::affected_rows);
                            save_query_history(
                                &history_store,
                                &history_tx,
                                &project,
                                cid,
                                &query_for_history,
                                QueryResultStatus::Success,
                                rows,
                            );
                        }
                        tx.send(Action::QueryCompleted {
                            dsn,
                            run_id,
                            result: Arc::new(result),
                            generation: 0,
                            target_page: None,
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        if let Some(cid) = &conn_id {
                            save_query_history(
                                &history_store,
                                &history_tx,
                                &project,
                                cid,
                                &query_for_history,
                                QueryResultStatus::Failed,
                                None,
                            );
                        }
                        tx.send(Action::QueryFailed {
                            dsn,
                            run_id,
                            error: e,
                            generation: 0,
                            source: QuerySource::Adhoc,
                        })
                        .await
                        .ok();
                    }
                }
            });
            Ok(())
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
            let history_tx = action_tx.clone();
            let project = state.runtime.project_name().to_string();
            let conn_id = state.session.active_connection_id().cloned();
            let query_for_history = query.clone();

            query_tasks.spawn(async move {
                match executor.execute_write(&dsn, &query, access_mode).await {
                    Ok(result) => {
                        if let Some(cid) = &conn_id {
                            save_query_history(
                                &history_store,
                                &history_tx,
                                &project,
                                cid,
                                &query_for_history,
                                QueryResultStatus::Success,
                                Some(result.affected_rows as u64),
                            );
                        }
                        tx.send(Action::ExecuteWriteSucceeded {
                            dsn,
                            run_id,
                            affected_rows: result.affected_rows,
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        if let Some(cid) = &conn_id {
                            save_query_history(
                                &history_store,
                                &history_tx,
                                &project,
                                cid,
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
            });
            Ok(())
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

            query_tasks.spawn(async move {
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
            });
            Ok(())
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
            let path = resolve_export_path(&file_name);
            let export_dsn = dsn.clone();

            query_tasks.spawn(async move {
                let result = export_to_path(path.clone(), |temporary_path| async move {
                    executor
                        .export_to_csv(&export_dsn, &query, &temporary_path)
                        .await
                })
                .await;
                match result {
                    Ok(_) => {
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
            });
            Ok(())
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
            let path = resolve_export_path(&file_name);
            let exported_path = path.display().to_string();

            query_tasks.spawn(async move {
                let result = export_to_path(path, |temporary_path| async move {
                    exporter
                        .export_cached_result_to_csv(temporary_path, columns, values)
                        .await
                })
                .await;

                match result {
                    Ok(_) => {
                        tx.send(Action::CsvExportSucceeded {
                            dsn,
                            run_id,
                            path: exported_path,
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
            });
            Ok(())
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
    use crate::domain::WriteExecutionResult;
    use crate::model::app_state::AppState;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{AccessMode, RenderOutput, RenderResult, Renderer};
    use crate::services::AppServices;
    use crate::update::action::Action;

    use super::{epoch_days_to_ymd, export_to_path, resolve_export_path};

    mod export_path {
        use std::path::Path;

        use super::*;

        #[test]
        fn epoch_days_to_ymd_unix_epoch() {
            assert_eq!(epoch_days_to_ymd(0), (1970, 1, 1));
        }

        #[test]
        fn epoch_days_to_ymd_known_date() {
            assert_eq!(epoch_days_to_ymd(19723), (2024, 1, 1));
        }

        #[test]
        fn epoch_days_to_ymd_leap_year_feb_29() {
            assert_eq!(epoch_days_to_ymd(19782), (2024, 2, 29));
        }

        #[test]
        fn epoch_days_to_ymd_year_end_dec_31() {
            assert_eq!(epoch_days_to_ymd(19722), (2023, 12, 31));
        }

        #[test]
        fn epoch_days_to_ymd_century_leap_year() {
            assert_eq!(epoch_days_to_ymd(11016), (2000, 2, 29));
        }

        #[test]
        fn epoch_days_to_ymd_non_leap_century() {
            assert_eq!(epoch_days_to_ymd(-25508), (1900, 3, 1));
        }

        #[test]
        fn resolve_export_path_contains_file_name() {
            let path = resolve_export_path("users");
            let file_name = path.file_name().unwrap().to_str().unwrap();
            assert!(file_name.starts_with("sabiql_export_users_"));
            assert!(
                Path::new(file_name)
                    .extension()
                    .is_some_and(|ext: &std::ffi::OsStr| ext.eq_ignore_ascii_case("csv"))
            );
        }
    }

    mod atomic_export {
        use std::future::pending;

        use crate::ports::outbound::DbOperationError;
        use tempfile::tempdir;
        use tokio::sync::oneshot;

        use super::*;

        #[tokio::test]
        async fn cancellation_removes_partial_temporary_file() {
            let dir = tempdir().unwrap();
            let final_path = dir.path().join("export.csv");
            let (started_tx, started_rx) = oneshot::channel();

            let task = tokio::spawn(export_to_path(
                final_path.clone(),
                move |temporary_path| async move {
                    tokio::fs::write(temporary_path, b"partial,csv\n")
                        .await
                        .unwrap();
                    started_tx.send(()).ok();
                    pending::<Result<usize, DbOperationError>>().await
                },
            ));

            started_rx.await.unwrap();
            task.abort();
            task.await.unwrap_err();

            assert!(!final_path.exists());
            assert_eq!(dir.path().read_dir().unwrap().count(), 0);
        }

        #[tokio::test]
        async fn success_renames_temporary_file_atomically() {
            let dir = tempdir().unwrap();
            let final_path = dir.path().join("export.csv");

            let row_count = export_to_path(final_path.clone(), |temporary_path| async move {
                tokio::fs::write(temporary_path, b"complete,csv\n")
                    .await
                    .unwrap();
                Ok(3)
            })
            .await
            .unwrap();

            assert_eq!(row_count, 3);
            assert_eq!(
                tokio::fs::read_to_string(&final_path).await.unwrap(),
                "complete,csv\n"
            );
            assert_eq!(dir.path().read_dir().unwrap().count(), 1);
        }
    }

    mod explain_plan_text {
        use crate::domain::{QueryResult, QuerySource, sqlite_explain_query_plan_text_from_result};

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
                sqlite_explain_query_plan_text_from_result(&result),
                "SEARCH users USING INDEX idx_users_name\n  - SCAN orders"
            );
        }

        #[test]
        fn non_sqlite_plan_keeps_first_column_fallback() {
            let result = QueryResult::success(
                "EXPLAIN SELECT * FROM users".to_string(),
                vec!["QUERY PLAN".to_string()],
                vec![vec!["Seq Scan on users".to_string()]],
                1,
                QuerySource::Adhoc,
            );

            assert_eq!(
                sqlite_explain_query_plan_text_from_result(&result),
                "Seq Scan on users"
            );
        }
    }
    mod cached_csv_export_effect {
        use std::cell::RefCell;
        use std::path::PathBuf;
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
        use crate::ports::outbound::{
            CachedResultExporter, DbOperationError, RenderOutput, RenderResult, Renderer,
        };
        use crate::services::AppServices;
        use crate::update::action::Action;

        struct NoopRenderer;
        impl Renderer for NoopRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: std::time::Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput::default())
            }
        }

        fn test_file_name(label: &str) -> String {
            format!("cached_{label}_{}", std::process::id())
        }

        struct FailingCachedResultExporter;

        #[async_trait::async_trait]
        impl CachedResultExporter for FailingCachedResultExporter {
            async fn export_cached_result_to_csv(
                &self,
                _path: PathBuf,
                _columns: Vec<String>,
                _values: Vec<Vec<QueryValue>>,
            ) -> Result<usize, DbOperationError> {
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
            let mut state = AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExportCsvFromCache {
                        dsn: "sqlite:///tmp/test.db".to_string(),
                        run_id: 7,
                        file_name: test_file_name("success"),
                        columns: vec!["id".to_string(), "payload".to_string()],
                        values: vec![vec![
                            QueryValue::SqlLiteral("1".to_string()),
                            QueryValue::Blob(vec![0xAB, 0xCD]),
                        ]],
                        row_count: Some(1),
                    }],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
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
            let mut state = AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExportCsvFromCache {
                        dsn: "sqlite:///tmp/test.db".to_string(),
                        run_id: 8,
                        file_name: test_file_name("failure"),
                        columns: vec!["id".to_string()],
                        values: vec![vec![QueryValue::SqlLiteral("1".to_string())]],
                        row_count: Some(1),
                    }],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(matches!(action, Action::CsvExportFailed { run_id: 8, .. }));
        }
    }

    mod execute_preview {
        use std::cell::RefCell;
        use std::sync::Arc;

        use tokio::sync::mpsc;

        use crate::cmd::cache::TtlCache;
        use crate::cmd::completion_engine::CompletionEngine;
        use crate::cmd::effect::Effect;
        use crate::cmd::test_fixtures;
        use std::time::Instant;

        use crate::domain::QuerySource;
        use crate::model::app_state::AppState;
        use crate::ports::outbound::connection_store::MockConnectionStore;
        use crate::ports::outbound::metadata::MockMetadataProvider;
        use crate::ports::outbound::query_executor::MockQueryExecutor;
        use crate::ports::outbound::{DbOperationError, RenderOutput, RenderResult, Renderer};
        use crate::services::AppServices;
        use crate::update::action::Action;

        struct NoopRenderer;
        impl Renderer for NoopRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput::default())
            }
        }

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

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExecutePreview {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        run_id: 8,
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
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

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExecutePreview {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        run_id: 8,
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(
                    action,
                    Action::QueryFailed {
                        source: QuerySource::Preview,
                        ..
                    }
                ),
                "expected QueryFailed, got {action:?}"
            );
        }
    }

    mod execute_access_mode {
        use super::*;

        struct NoopRenderer;
        impl Renderer for NoopRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: std::time::Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput::default())
            }
        }

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
            let mut state = AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![effect],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            tokio::time::timeout(Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed")
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
        async fn execute_write_forwards_access_mode() {
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_write()
                .once()
                .withf(|_, _, access_mode| *access_mode == AccessMode::ReadWrite)
                .returning(|_, _, _| {
                    Ok(WriteExecutionResult {
                        affected_rows: 1,
                        execution_time_ms: 0,
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

            assert!(matches!(
                action,
                Action::ExecuteWriteSucceeded {
                    run_id: 3,
                    affected_rows: 1,
                    ..
                }
            ));
        }
    }
}
