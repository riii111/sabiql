use std::cell::RefCell;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::effect::Effect;
use crate::cmd::metadata_task::MetadataTaskRegistry;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::cmd::sqlite_path_validate::validate_sqlite_database_path;
use crate::domain::sqlite_path_from_dsn;
use crate::policy::sqlite_path::to_db_operation_error;
use crate::ports::outbound::{DbOperationError, MetadataProvider, SqlitePathValidator};
use crate::update::action::Action;

pub(in crate::cmd) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    sqlite_path_validator: &Arc<dyn SqlitePathValidator>,
    table_detail_tasks: &SingleTaskOwner,
    metadata_tasks: &Arc<MetadataTaskRegistry>,
    completion_engine: &RefCell<CompletionEngine>,
) {
    match effect {
        Effect::CancelMetadataTasks => {
            metadata_tasks.cancel().await;
        }
        Effect::FetchMetadata { dsn, run_id } => {
            fetch_metadata(
                action_tx,
                metadata_provider,
                sqlite_path_validator,
                metadata_tasks,
                dsn,
                run_id,
            )
            .await;
        }
        Effect::FetchEffectiveUser { dsn, run_id } => {
            fetch_effective_user(action_tx, metadata_provider, metadata_tasks, dsn, run_id);
        }
        Effect::FetchTableDetail {
            dsn,
            schema,
            table,
            generation,
            run_id,
        } => {
            fetch_table_detail(
                action_tx,
                metadata_provider,
                dsn,
                schema,
                table,
                generation,
                run_id,
                table_detail_tasks,
            )
            .await;
        }
        Effect::PrefetchTableColumnsAndFks {
            dsn,
            run_id,
            schema,
            table,
        } => {
            prefetch_table_detail(
                action_tx,
                metadata_provider,
                completion_engine,
                metadata_tasks,
                dsn,
                run_id,
                schema,
                table,
            )
            .await;
        }
        Effect::SchedulePrefetchQueueProcessing { run_id } => {
            action_tx
                .send(Action::ProcessPrefetchQueue { run_id })
                .await
                .ok();
        }
        Effect::DelayedProcessPrefetchQueue { run_id, delay_secs } => {
            let tx = action_tx.clone();
            MetadataTaskRegistry::spawn(metadata_tasks, async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                tx.send(Action::ProcessPrefetchQueue { run_id }).await.ok();
            });
        }
        _ => unreachable!("metadata::run called with non-metadata effect"),
    }
}

async fn fetch_metadata(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    sqlite_path_validator: &Arc<dyn SqlitePathValidator>,
    metadata_tasks: &Arc<MetadataTaskRegistry>,
    dsn: String,
    run_id: u64,
) {
    if let Some(path) = sqlite_path_from_dsn(&dsn)
        && let Err(error) =
            validate_sqlite_database_path(sqlite_path_validator, path.to_string()).await
    {
        action_tx
            .send(Action::MetadataFailed {
                run_id,
                error: to_db_operation_error(&error),
            })
            .await
            .ok();
        return;
    }

    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    MetadataTaskRegistry::spawn(metadata_tasks, async move {
        match provider.fetch_metadata(&dsn).await {
            Ok(metadata) => {
                let metadata = Arc::new(metadata);
                tx.send(Action::MetadataLoaded { run_id, metadata })
                    .await
                    .ok();
            }
            Err(e) => {
                tx.send(Action::MetadataFailed { run_id, error: e })
                    .await
                    .ok();
            }
        }
    });
}

fn fetch_effective_user(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_tasks: &Arc<MetadataTaskRegistry>,
    dsn: String,
    run_id: u64,
) {
    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    MetadataTaskRegistry::spawn(metadata_tasks, async move {
        let effective_user = provider.fetch_effective_user(&dsn).await.ok().flatten();
        tx.send(Action::EffectiveUserLoaded {
            run_id,
            effective_user,
        })
        .await
        .ok();
    });
}

async fn fetch_table_detail(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    dsn: String,
    schema: String,
    table: String,
    generation: u64,
    run_id: u64,
    table_detail_tasks: &SingleTaskOwner,
) {
    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    table_detail_tasks
        .replace(async move {
            let outcome = provider
                .fetch_table_detail(&dsn, &schema, &table)
                .await
                .map(Box::new);
            tx.send(Action::TableDetailLoaded {
                dsn,
                run_id,
                outcome,
                generation,
            })
            .await
            .ok();
        })
        .await;
}

async fn prefetch_table_detail(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    completion_engine: &RefCell<CompletionEngine>,
    metadata_tasks: &Arc<MetadataTaskRegistry>,
    dsn: String,
    run_id: u64,
    schema: String,
    table: String,
) {
    let qualified_name = format!("{schema}.{table}");
    let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);

    if already_cached {
        action_tx
            .send(Action::TableDetailCached {
                dsn,
                run_id,
                schema,
                table,
                detail: None,
            })
            .await
            .ok();
        return;
    }

    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    MetadataTaskRegistry::spawn(metadata_tasks, async move {
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            provider.fetch_table_columns_and_fks(&dsn, &schema, &table),
        )
        .await;
        match result {
            Ok(Ok(detail)) => {
                tx.send(Action::TableDetailCached {
                    dsn,
                    run_id,
                    schema,
                    table,
                    detail: Some(Box::new(detail)),
                })
                .await
                .ok();
            }
            Ok(Err(e)) => {
                tx.send(Action::TableDetailCacheFailed {
                    dsn,
                    run_id,
                    schema,
                    table,
                    error: e,
                })
                .await
                .ok();
            }
            Err(_) => {
                tx.send(Action::TableDetailCacheFailed {
                    dsn,
                    run_id,
                    schema,
                    table,
                    error: DbOperationError::Timeout("timed out".to_string()),
                })
                .await
                .ok();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures;

    use crate::domain::SqlitePathError;
    use crate::model::app_state::AppState;
    use crate::ports::outbound::DbOperationError;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::update::action::Action;

    mod fetch_metadata {
        use super::*;

        #[tokio::test]
        async fn sqlite_missing_file_fails_before_provider_call() {
            use tempfile::tempdir;

            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let dsn = format!("sqlite://{}", path.display());
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().never();

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchMetadata {
                    dsn: dsn.clone(),
                    run_id: 7,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(200)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::MetadataFailed {
                        run_id: 7,
                        error: DbOperationError::SqlitePath(SqlitePathError::FileNotFound(
                            ref file_path,
                        )),
                    } if file_path == &path.display().to_string()
                ),
                "expected MetadataFailed, got {action:?}"
            );
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn sqlite_existing_file_reaches_provider() {
            use std::fs;
            use tempfile::tempdir;

            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();
            let dsn = format!("sqlite://{}", path.display());
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(test_fixtures::sample_metadata()));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchMetadata {
                    dsn: dsn.clone(),
                    run_id: 7,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::MetadataLoaded { run_id: 7, .. }),
                "expected MetadataLoaded, got {action:?}"
            );
        }

        #[tokio::test]
        async fn fetches_metadata_and_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(test_fixtures::sample_metadata()));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchMetadata {
                    dsn: "dsn://miss".to_string(),
                    run_id: 7,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::MetadataLoaded { run_id: 7, .. }),
                "expected MetadataLoaded, got {action:?}"
            );
        }

        #[tokio::test]
        async fn provider_error_returns_metadata_failed() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Err(DbOperationError::ConnectionFailed("timeout".to_string())));
            mock_provider.expect_fetch_effective_user().never();

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchMetadata {
                    dsn: "dsn://err".to_string(),
                    run_id: 7,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::MetadataFailed { run_id: 7, .. }),
                "expected MetadataFailed, got {action:?}"
            );
        }

        #[tokio::test]
        async fn effective_user_provider_error_is_reported_as_absent() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_effective_user()
                .once()
                .returning(|_| {
                    Err(DbOperationError::QueryFailed(
                        "permission denied".to_string(),
                    ))
                });

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchEffectiveUser {
                    dsn: "dsn://test".to_string(),
                    run_id: 7,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::EffectiveUserLoaded {
                    run_id: 7,
                    effective_user: None,
                }
            ));
        }
    }

    mod table_detail_dispatch {
        use crate::test_support;

        use super::*;
        use crate::domain::Table;

        fn sample_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                ..test_support::table::minimal("", "")
            }
        }

        #[tokio::test]
        async fn fetch_table_detail_calls_full_provider() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_table_detail()
                .once()
                .returning(|_, _, _| Ok(sample_table()));
            mock_provider.expect_fetch_table_columns_and_fks().never();

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchTableDetail {
                    dsn: "dsn://test".to_string(),
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    generation: 1,
                    run_id: 9,
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::TableDetailLoaded {
                        ref dsn,
                        run_id: 9,
                        generation: 1,
                        outcome: Ok(_),
                        ..
                    } if dsn == "dsn://test"
                ),
                "expected TableDetailLoaded, got {action:?}"
            );
        }

        #[tokio::test]
        async fn prefetch_table_detail_calls_light_provider() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_table_detail().never();
            mock_provider
                .expect_fetch_table_columns_and_fks()
                .once()
                .returning(|_, _, _| Ok(sample_table()));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::PrefetchTableColumnsAndFks {
                    dsn: "dsn://test".to_string(),
                    run_id: 3,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::TableDetailCached {
                        detail: Some(_),
                        ..
                    }
                ),
                "expected TableDetailCached, got {action:?}"
            );
        }

        #[tokio::test]
        async fn cached_table_detail_returns_empty_outcome() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_table_detail().never();
            mock_provider.expect_fetch_table_columns_and_fks().never();

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );
            let mut completion_engine = CompletionEngine::new();
            completion_engine.cache_table_detail("public.users".to_string(), sample_table());

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::PrefetchTableColumnsAndFks {
                    dsn: "dsn://test".to_string(),
                    run_id: 3,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                },
                AppState::new("test".to_string()),
                RefCell::new(completion_engine),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::TableDetailCached {
                        ref dsn,
                        run_id: 3,
                        detail: None,
                        ..
                    } if dsn == "dsn://test"
                ),
                "expected cache-hit TableDetailCached, got {action:?}"
            );
        }
    }
}
