use std::cell::RefCell;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::cache::TtlCache;
use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::effect::Effect;
use crate::cmd::metadata_task::MetadataTaskRegistry;
use crate::cmd::query_task::TableDetailTaskRegistry;
use crate::cmd::sqlite_path_validate::validate_sqlite_database_path;
use crate::domain::DatabaseMetadata;
use crate::domain::sqlite_path_from_dsn;
use crate::policy::sqlite_path::to_db_operation_error;
use crate::ports::outbound::{DbOperationError, MetadataProvider, SqlitePathValidator};
use crate::update::action::Action;

pub async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, Arc<DatabaseMetadata>>,
    sqlite_path_validator: &Arc<dyn SqlitePathValidator>,
    table_detail_tasks: &TableDetailTaskRegistry,
    metadata_tasks: &Arc<MetadataTaskRegistry>,
    completion_engine: &RefCell<CompletionEngine>,
) {
    match effect {
        Effect::FetchMetadata { dsn, run_id } => {
            fetch_metadata(
                action_tx,
                metadata_provider,
                metadata_cache,
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
        Effect::CacheInvalidate { dsn } => {
            metadata_cache.invalidate(&dsn).await;
        }

        _ => unreachable!("metadata::run called with non-metadata effect"),
    }
}

async fn fetch_metadata(
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, Arc<DatabaseMetadata>>,
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
                dsn,
                run_id,
                error: to_db_operation_error(&error),
            })
            .await
            .ok();
        return;
    }

    if let Some(cached) = metadata_cache.get(&dsn).await {
        action_tx
            .send(Action::MetadataLoaded {
                dsn,
                run_id,
                metadata: cached,
            })
            .await
            .ok();
        return;
    }

    let provider = Arc::clone(metadata_provider);
    let cache = metadata_cache.clone();
    let tx = action_tx.clone();

    MetadataTaskRegistry::spawn(metadata_tasks, async move {
        match provider.fetch_metadata(&dsn).await {
            Ok(metadata) => {
                let metadata = Arc::new(metadata);
                cache.set(dsn.clone(), Arc::clone(&metadata)).await;
                tx.send(Action::MetadataLoaded {
                    dsn,
                    run_id,
                    metadata,
                })
                .await
                .ok();
            }
            Err(e) => {
                tx.send(Action::MetadataFailed {
                    dsn,
                    run_id,
                    error: e,
                })
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
            dsn,
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
    table_detail_tasks: &TableDetailTaskRegistry,
) {
    let provider = Arc::clone(metadata_provider);
    let tx = action_tx.clone();

    table_detail_tasks
        .spawn(async move {
            match provider.fetch_table_detail(&dsn, &schema, &table).await {
                Ok(detail) => {
                    tx.send(Action::TableDetailLoaded {
                        dsn,
                        run_id,
                        detail: Box::new(detail),
                        generation,
                    })
                    .await
                    .ok();
                }
                Err(e) => {
                    tx.send(Action::TableDetailFailed {
                        dsn,
                        run_id,
                        error: e,
                        generation,
                    })
                    .await
                    .ok();
                }
            }
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
            .send(Action::TableDetailAlreadyCached {
                dsn,
                run_id,
                schema,
                table,
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
                    detail: Box::new(detail),
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

    use crate::cmd::cache::TtlCache;
    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures;

    use crate::domain::{DatabaseMetadata, SqlitePathError};
    use crate::model::app_state::AppState;
    use crate::ports::outbound::DbOperationError;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::update::action::Action;

    mod fetch_metadata {
        use super::*;

        #[tokio::test]
        async fn cache_hit_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().never();

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            cache
                .set(
                    "dsn://test".to_string(),
                    Arc::new(test_fixtures::sample_metadata()),
                )
                .await;

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::FetchMetadata {
                    dsn: "dsn://test".to_string(),
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
                    Action::MetadataLoaded {
                        ref dsn,
                        run_id: 7,
                        ..
                    } if dsn == "dsn://test"
                ),
                "expected MetadataLoaded, got {action:?}"
            );
        }

        #[tokio::test]
        async fn sqlite_missing_file_fails_before_provider_call() {
            use tempfile::tempdir;

            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let dsn = format!("sqlite://{}", path.display());
            let expected_dsn = dsn.clone();

            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().never();

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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
                        ref dsn,
                        run_id: 7,
                        error: DbOperationError::SqlitePath(SqlitePathError::FileNotFound(
                            ref file_path,
                        )),
                    } if *dsn == expected_dsn && file_path == &path.display().to_string()
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
            let expected_dsn = dsn.clone();

            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(test_fixtures::sample_metadata()));

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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
                matches!(
                    action,
                    Action::MetadataLoaded {
                        ref dsn,
                        run_id: 7,
                        ..
                    } if *dsn == expected_dsn
                ),
                "expected MetadataLoaded, got {action:?}"
            );
        }

        #[tokio::test]
        async fn cache_miss_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(test_fixtures::sample_metadata()));

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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
                matches!(
                    action,
                    Action::MetadataLoaded {
                        ref dsn,
                        run_id: 7,
                        ..
                    } if dsn == "dsn://miss"
                ),
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

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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
                matches!(
                    action,
                    Action::MetadataFailed {
                        ref dsn,
                        run_id: 7,
                        ..
                    } if dsn == "dsn://err"
                ),
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
                TtlCache::new(300),
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
                    ref dsn,
                    run_id: 7,
                    effective_user: None,
                } if dsn == "dsn://test"
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

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
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
                matches!(action, Action::TableDetailCached { .. }),
                "expected TableDetailCached, got {action:?}"
            );
        }
    }

    mod cache_invalidate {
        use super::*;

        #[tokio::test]
        async fn invalidate_removes_cache_entry() {
            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            cache
                .set(
                    "dsn://target".to_string(),
                    Arc::new(test_fixtures::sample_metadata()),
                )
                .await;

            assert!(cache.get(&"dsn://target".to_string()).await.is_some());

            let (tx, mut _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache.clone(),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::CacheInvalidate {
                    dsn: "dsn://target".to_string(),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut _rx,
                None,
            )
            .await
            .unwrap();

            assert_eq!(run.state.runtime.project_name(), "test");
            assert!(cache.get(&"dsn://target".to_string()).await.is_none());
        }

        #[tokio::test]
        async fn invalidate_forces_following_metadata_fetch() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(test_fixtures::sample_metadata()));

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            cache
                .set(
                    "dsn://target".to_string(),
                    Arc::new(DatabaseMetadata::new("old".to_string())),
                )
                .await;

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache.clone(),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::Sequence(vec![
                    Effect::CacheInvalidate {
                        dsn: "dsn://target".to_string(),
                    },
                    Effect::FetchMetadata {
                        dsn: "dsn://target".to_string(),
                        run_id: 7,
                    },
                ]),
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
                Action::MetadataLoaded {
                    ref dsn,
                    run_id: 7,
                    ref metadata,
                } if dsn == "dsn://target" && metadata.database_name == "testdb"
            ));
            assert_eq!(
                cache
                    .get(&"dsn://target".to_string())
                    .await
                    .expect("fetched metadata should be cached")
                    .database_name,
                "testdb"
            );
        }
    }
}
