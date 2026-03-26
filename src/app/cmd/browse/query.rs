use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::ports::{DbOperationError, QueryExecutor, QueryHistoryStore};
use crate::app::update::action::Action;
use crate::domain::ConnectionId;
use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};

fn epoch_days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
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
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
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
    let timestamp = format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}_{:03}",
        y, m, d, hours, minutes, seconds, millis
    );
    let file_stem = format!("sabiql_export_{}_{}.csv", file_name, timestamp);
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(file_stem)
}

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_executor: &Arc<dyn QueryExecutor>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
    _state: &mut AppState,
) -> Result<()> {
    match effect {
        Effect::ExecutePreview {
            dsn,
            schema,
            table,
            generation,
            limit,
            offset,
            target_page,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                match executor
                    .execute_preview(&dsn, &schema, &table, limit, offset, read_only)
                    .await
                {
                    Ok(result) => {
                        tx.send(Action::QueryCompleted {
                            result: Arc::new(result),
                            generation,
                            target_page: Some(target_page),
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        tx.send(Action::QueryFailed(e, generation)).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ExecuteExplain {
            dsn,
            query,
            is_analyze,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                let start = Instant::now();
                match executor.execute_adhoc(&dsn, &query, read_only).await {
                    Ok(result) => {
                        let elapsed = start.elapsed().as_millis() as u64;
                        if result.is_error() {
                            let error_text = result
                                .rows
                                .iter()
                                .filter_map(|row| row.first())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("\n");
                            let error_text = if error_text.is_empty() {
                                "EXPLAIN failed".to_string()
                            } else {
                                error_text
                            };
                            tx.send(Action::ExplainFailed(DbOperationError::QueryFailed(
                                error_text,
                            )))
                            .await
                            .ok();
                        } else {
                            let plan_text = result
                                .rows
                                .iter()
                                .filter_map(|row| row.first())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("\n");
                            tx.send(Action::ExplainCompleted {
                                plan_text,
                                is_analyze,
                                execution_time_ms: elapsed,
                            })
                            .await
                            .ok();
                        }
                    }
                    Err(e) => {
                        tx.send(Action::ExplainFailed(e)).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ExecuteAdhoc {
            dsn,
            query,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let history_store = Arc::clone(query_history_store);
            let history_tx = action_tx.clone();
            let project = _state.runtime.project_name.clone();
            let conn_id = _state.session.active_connection_id.clone();
            let query_for_history = query.clone();

            tokio::spawn(async move {
                match executor.execute_adhoc(&dsn, &query, read_only).await {
                    Ok(result) => {
                        if let Some(cid) = &conn_id {
                            let (status, rows) = if result.is_error() {
                                (QueryResultStatus::Failed, None)
                            } else {
                                let rows =
                                    result.command_tag.as_ref().and_then(|t| t.affected_rows());
                                (QueryResultStatus::Success, rows)
                            };
                            save_query_history(
                                &history_store,
                                &history_tx,
                                &project,
                                cid,
                                &query_for_history,
                                status,
                                rows,
                            );
                        }
                        tx.send(Action::QueryCompleted {
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
                        tx.send(Action::QueryFailed(e, 0)).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ExecuteWrite {
            dsn,
            query,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let history_store = Arc::clone(query_history_store);
            let history_tx = action_tx.clone();
            let project = _state.runtime.project_name.clone();
            let conn_id = _state.session.active_connection_id.clone();
            let query_for_history = query.clone();

            tokio::spawn(async move {
                match executor.execute_write(&dsn, &query, read_only).await {
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
                        tx.send(Action::ExecuteWriteFailed(e)).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::CountRowsForExport {
            dsn,
            count_query,
            export_query,
            file_name,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                let row_count = executor
                    .count_query_rows(&dsn, &count_query, read_only)
                    .await
                    .ok();
                tx.send(Action::CsvExportRowsCounted {
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
            query,
            file_name,
            row_count,
            read_only,
        } => {
            let executor = Arc::clone(query_executor);
            let tx = action_tx.clone();
            let path = resolve_export_path(&file_name);

            tokio::spawn(async move {
                match executor.export_to_csv(&dsn, &query, &path, read_only).await {
                    Ok(_) => {
                        tx.send(Action::CsvExportSucceeded {
                            path: path.display().to_string(),
                            row_count,
                        })
                        .await
                        .ok();
                    }
                    Err(e) => {
                        tx.send(Action::CsvExportFailed(e)).await.ok();
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
    use super::{epoch_days_to_ymd, resolve_export_path};

    mod export_path {
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
            assert!(file_name.ends_with(".csv"));
        }
    }

    mod execute_preview {
        use std::cell::RefCell;
        use std::sync::Arc;

        use tokio::sync::mpsc;

        use crate::app::cmd::cache::TtlCache;
        use crate::app::cmd::completion_engine::CompletionEngine;
        use crate::app::cmd::effect::Effect;
        use crate::app::cmd::test_support::*;
        use std::time::Instant;

        use crate::app::model::app_state::AppState;
        use crate::app::ports::connection_store::MockConnectionStore;
        use crate::app::ports::metadata::MockMetadataProvider;
        use crate::app::ports::query_executor::MockQueryExecutor;
        use crate::app::ports::{DbOperationError, RenderOutput, Renderer};
        use crate::app::services::AppServices;
        use crate::app::update::action::Action;
        use color_eyre::eyre::Result;

        struct NoopRenderer;
        impl Renderer for NoopRenderer {
            fn draw(
                &mut self,
                _state: &mut AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> Result<RenderOutput> {
                Ok(RenderOutput::default())
            }
        }

        #[tokio::test]
        async fn success_returns_query_completed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _, _| Ok(sample_query_result()));

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
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
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                        read_only: false,
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
                "expected QueryCompleted, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn error_returns_query_failed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _, _| {
                    Err(DbOperationError::QueryFailed("syntax error".to_string()))
                });

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
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
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                        read_only: false,
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
                matches!(action, Action::QueryFailed(_, _)),
                "expected QueryFailed, got {:?}",
                action
            );
        }
    }
}
