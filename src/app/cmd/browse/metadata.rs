use std::cell::RefCell;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::cache::TtlCache;
use crate::app::cmd::completion_engine::CompletionEngine;
use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::ports::{DbOperationError, MetadataProvider};
use crate::app::update::action::Action;
use crate::domain::DatabaseMetadata;

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, Arc<DatabaseMetadata>>,
    _state: &mut AppState,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match effect {
        Effect::FetchMetadata { dsn } => {
            if let Some(cached) = metadata_cache.get(&dsn).await {
                action_tx.send(Action::MetadataLoaded(cached)).await.ok();
            } else {
                let provider = Arc::clone(metadata_provider);
                let cache = metadata_cache.clone();
                let tx = action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_metadata(&dsn).await {
                        Ok(metadata) => {
                            let metadata = Arc::new(metadata);
                            cache.set(dsn, Arc::clone(&metadata)).await;
                            tx.send(Action::MetadataLoaded(metadata)).await.ok();
                        }
                        Err(e) => {
                            tx.send(Action::MetadataFailed(e)).await.ok();
                        }
                    }
                });
            }
            Ok(())
        }

        Effect::FetchTableDetail {
            dsn,
            schema,
            table,
            generation,
        } => {
            let provider = Arc::clone(metadata_provider);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                match provider.fetch_table_detail(&dsn, &schema, &table).await {
                    Ok(detail) => {
                        tx.send(Action::TableDetailLoaded(Box::new(detail), generation))
                            .await
                            .ok();
                    }
                    Err(e) => {
                        tx.send(Action::TableDetailFailed(e, generation)).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::PrefetchTableDetail { dsn, schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);

            let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);
            if already_cached {
                action_tx
                    .send(Action::TableDetailAlreadyCached { schema, table })
                    .await
                    .ok();
                return Ok(());
            }

            let provider = Arc::clone(metadata_provider);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(10),
                    provider.fetch_table_columns_and_fks(&dsn, &schema, &table),
                )
                .await;
                match result {
                    Ok(Ok(detail)) => {
                        tx.send(Action::TableDetailCached {
                            schema,
                            table,
                            detail: Box::new(detail),
                        })
                        .await
                        .ok();
                    }
                    Ok(Err(e)) => {
                        tx.send(Action::TableDetailCacheFailed {
                            schema,
                            table,
                            error: e,
                        })
                        .await
                        .ok();
                    }
                    Err(_) => {
                        tx.send(Action::TableDetailCacheFailed {
                            schema,
                            table,
                            error: DbOperationError::Timeout,
                        })
                        .await
                        .ok();
                    }
                }
            });
            Ok(())
        }

        Effect::ProcessPrefetchQueue => {
            action_tx.send(Action::ProcessPrefetchQueue).await.ok();
            Ok(())
        }

        Effect::DelayedProcessPrefetchQueue { delay_secs } => {
            let tx = action_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                tx.send(Action::ProcessPrefetchQueue).await.ok();
            });
            Ok(())
        }

        Effect::CacheInvalidate { dsn } => {
            metadata_cache.invalidate(&dsn).await;
            Ok(())
        }

        _ => unreachable!("metadata::run called with non-metadata effect"),
    }
}

#[cfg(test)]
mod tests {
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
    use crate::domain::DatabaseMetadata;
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

    mod fetch_metadata {
        use super::*;

        #[tokio::test]
        async fn cache_hit_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().never();

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            cache
                .set("dsn://test".to_string(), Arc::new(sample_metadata()))
                .await;

            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://test".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::MetadataLoaded(_)),
                "expected MetadataLoaded, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn cache_miss_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(sample_metadata()));

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://miss".to_string(),
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
                matches!(action, Action::MetadataLoaded(_)),
                "expected MetadataLoaded, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn provider_error_returns_metadata_failed() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Err(DbOperationError::ConnectionFailed("timeout".to_string())));

            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://err".to_string(),
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
                matches!(action, Action::MetadataFailed(_)),
                "expected MetadataFailed, got {:?}",
                action
            );
        }
    }

    mod table_detail_dispatch {
        use super::*;
        use crate::domain::Table;

        fn sample_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
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
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchTableDetail {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
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
                matches!(action, Action::TableDetailLoaded(_, 1)),
                "expected TableDetailLoaded, got {:?}",
                action
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
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::PrefetchTableDetail {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
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
                matches!(action, Action::TableDetailCached { .. }),
                "expected TableDetailCached, got {:?}",
                action
            );
        }
    }

    mod cache_invalidate {
        use super::*;

        #[tokio::test]
        async fn invalidate_removes_cache_entry() {
            let cache: TtlCache<String, Arc<DatabaseMetadata>> = TtlCache::new(300);
            cache
                .set("dsn://target".to_string(), Arc::new(sample_metadata()))
                .await;

            assert!(cache.get(&"dsn://target".to_string()).await.is_some());

            let (tx, _rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache.clone(),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::CacheInvalidate {
                        dsn: "dsn://target".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(cache.get(&"dsn://target".to_string()).await.is_none());
        }
    }
}
