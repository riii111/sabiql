//! Executes side effects returned by the reducer.
//!
//! # RefCell Borrow Safety
//!
//! When effects need data from `completion_engine` (a `RefCell`), the borrow
//! MUST be dropped before any await point:
//!
//! ```ignore
//! let tables = {
//!     let engine = completion_engine.borrow();
//!     engine.table_details_iter().map(|...| ...).collect()
//! };  // borrow dropped here
//! some_async_operation(tables).await;  // safe
//! ```

use std::cell::RefCell;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::cache::TtlCache;
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::er_task::spawn_er_diagram_task;
use crate::app::ports::{
    ConfigWriter, ConnectionStore, ErDiagramExporter, ErLogWriter, MetadataProvider, QueryExecutor,
    Renderer,
};
use crate::app::state::AppState;
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, ErTableInfo};

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    er_exporter: Arc<dyn ErDiagramExporter>,
    config_writer: Arc<dyn ConfigWriter>,
    er_log_writer: Arc<dyn ErLogWriter>,
    connection_store: Arc<dyn ConnectionStore>,
    metadata_cache: TtlCache<String, DatabaseMetadata>,
    action_tx: mpsc::Sender<Action>,
}

impl EffectRunner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metadata_provider: Arc<dyn MetadataProvider>,
        query_executor: Arc<dyn QueryExecutor>,
        er_exporter: Arc<dyn ErDiagramExporter>,
        config_writer: Arc<dyn ConfigWriter>,
        er_log_writer: Arc<dyn ErLogWriter>,
        connection_store: Arc<dyn ConnectionStore>,
        metadata_cache: TtlCache<String, DatabaseMetadata>,
        action_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            metadata_provider,
            query_executor,
            er_exporter,
            config_writer,
            er_log_writer,
            connection_store,
            metadata_cache,
            action_tx,
        }
    }

    pub async fn run<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        self.run_single(seq_effect, tui, state, completion_engine)
                            .await?;
                    }
                }
                single_effect => {
                    self.run_single(single_effect, tui, state, completion_engine)
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn run_single<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        self.run_normal(effect, tui, state, completion_engine).await
    }

    async fn run_normal<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        match effect {
            Effect::Render => {
                let output = tui.draw(state)?;
                if !state.ui.focus_mode {
                    state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
                }
                state.ui.result_viewport_plan = output.result_viewport_plan;
                state.ui.explorer_pane_height = output.explorer_pane_height;
                state.ui.inspector_pane_height = output.inspector_pane_height;
                state.ui.result_pane_height = output.result_pane_height;
                Ok(())
            }

            Effect::SaveAndConnect {
                id,
                name,
                host,
                port,
                database,
                user,
                password,
                ssl_mode,
            } => {
                let profile = match id {
                    Some(existing_id) => {
                        // Edit mode: use existing ID
                        match ConnectionProfile::with_id(
                            existing_id,
                            name,
                            host,
                            port,
                            database,
                            user,
                            password,
                            ssl_mode,
                        ) {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = self
                                    .action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()));
                                return Ok(());
                            }
                        }
                    }
                    None => {
                        // New connection: generate new ID
                        match ConnectionProfile::new(
                            name, host, port, database, user, password, ssl_mode,
                        ) {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = self
                                    .action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()));
                                return Ok(());
                            }
                        }
                    }
                };
                let id = profile.id.clone();
                let dsn = profile.to_dsn();
                let name = profile.name.as_str().to_string();
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                let provider = Arc::clone(&self.metadata_provider);
                let cache = self.metadata_cache.clone();
                tokio::spawn(async move {
                    match provider.fetch_metadata(&dsn).await {
                        Ok(metadata) => {
                            cache.set(dsn.clone(), metadata).await;
                            match store.save(&profile) {
                                Ok(()) => {
                                    let _ = tx
                                        .send(Action::ConnectionSaveCompleted { id, dsn, name })
                                        .await;
                                }
                                Err(e) => {
                                    cache.invalidate(&dsn).await;
                                    let _ =
                                        tx.send(Action::ConnectionSaveFailed(e.to_string())).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Action::MetadataFailed(e.to_string())).await;
                        }
                    }
                });
                Ok(())
            }

            Effect::LoadConnectionForEdit { id } => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.find_by_id(&id) {
                    Ok(Some(profile)) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoaded(Box::new(profile)));
                    }
                    Ok(None) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoadFailed(
                            "Connection not found".to_string(),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoadFailed(e.to_string()));
                    }
                });
                Ok(())
            }

            Effect::LoadConnections => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.load_all() {
                    Ok(profiles) => {
                        let _ = tx.blocking_send(Action::ConnectionsLoaded(profiles));
                    }
                    Err(_) => {
                        // On error, send empty list to avoid blocking UI
                        let _ = tx.blocking_send(Action::ConnectionsLoaded(vec![]));
                    }
                });
                Ok(())
            }

            Effect::DeleteConnection { id } => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.delete(&id) {
                    Ok(()) => {
                        let _ = tx.blocking_send(Action::ConnectionDeleted(id));
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Action::ConnectionDeleteFailed(e.to_string()));
                    }
                });
                Ok(())
            }

            Effect::CacheInvalidate { dsn } => {
                self.metadata_cache.invalidate(&dsn).await;
                Ok(())
            }

            Effect::ClearCompletionEngineCache => {
                completion_engine.borrow_mut().clear_table_cache();
                Ok(())
            }

            Effect::FetchMetadata { dsn } => {
                if let Some(cached) = self.metadata_cache.get(&dsn).await {
                    let _ = self
                        .action_tx
                        .send(Action::MetadataLoaded(Box::new(cached)))
                        .await;
                } else {
                    let provider = Arc::clone(&self.metadata_provider);
                    let cache = self.metadata_cache.clone();
                    let tx = self.action_tx.clone();

                    tokio::spawn(async move {
                        match provider.fetch_metadata(&dsn).await {
                            Ok(metadata) => {
                                cache.set(dsn, metadata.clone()).await;
                                let _ = tx.send(Action::MetadataLoaded(Box::new(metadata))).await;
                            }
                            Err(e) => {
                                let _ = tx.send(Action::MetadataFailed(e.to_string())).await;
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
                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailLoaded(Box::new(detail), generation))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::PrefetchTableDetail { dsn, schema, table } => {
                let qualified_name = format!("{}.{}", schema, table);

                let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);
                if already_cached {
                    let _ = self
                        .action_tx
                        .send(Action::TableDetailAlreadyCached { schema, table })
                        .await;
                    return Ok(());
                }

                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailCached {
                                    schema,
                                    table,
                                    detail: Box::new(detail),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailCacheFailed {
                                    schema,
                                    table,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::ProcessPrefetchQueue => {
                let _ = self.action_tx.send(Action::ProcessPrefetchQueue).await;
                Ok(())
            }

            Effect::ExecutePreview {
                dsn,
                schema,
                table,
                generation,
                limit,
                offset,
                target_page,
            } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor
                        .execute_preview(&dsn, &schema, &table, limit, offset)
                        .await
                    {
                        Ok(result) => {
                            let _ = tx
                                .send(Action::QueryCompleted {
                                    result: Arc::new(result),
                                    generation,
                                    target_page: Some(target_page),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::QueryFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::ExecuteAdhoc { dsn, query } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor.execute_adhoc(&dsn, &query).await {
                        Ok(result) => {
                            let _ = tx
                                .send(Action::QueryCompleted {
                                    result: Arc::new(result),
                                    generation: 0,
                                    target_page: None,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx.send(Action::QueryFailed(e.to_string(), 0)).await;
                        }
                    }
                });
                Ok(())
            }

            Effect::ExecuteWrite { dsn, query } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor.execute_write(&dsn, &query).await {
                        Ok(result) => {
                            let _ = tx
                                .send(Action::ExecuteWriteSucceeded {
                                    affected_rows: result.affected_rows,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx.send(Action::ExecuteWriteFailed(e.to_string())).await;
                        }
                    }
                });
                Ok(())
            }

            Effect::CacheTableInCompletionEngine {
                qualified_name,
                table,
            } => {
                completion_engine
                    .borrow_mut()
                    .cache_table_detail(qualified_name, *table);
                Ok(())
            }

            Effect::TriggerCompletion => {
                let cursor = state.sql_modal.cursor;

                let missing = {
                    let engine = completion_engine.borrow();
                    engine.missing_tables(&state.sql_modal.content, state.cache.metadata.as_ref())
                };

                let prefetch_actions: Vec<Action> = missing
                    .into_iter()
                    .filter_map(|qualified_name| {
                        qualified_name.split_once('.').map(|(schema, table)| {
                            Action::PrefetchTableDetail {
                                schema: schema.to_string(),
                                table: table.to_string(),
                            }
                        })
                    })
                    .collect();

                for action in prefetch_actions {
                    let _ = self.action_tx.try_send(action);
                }

                let (candidates, token_len, visible) = {
                    let engine = completion_engine.borrow();
                    let token_len = engine.current_token_len(&state.sql_modal.content, cursor);
                    let recent_cols = state.sql_modal.completion.recent_columns_vec();
                    let candidates = engine.get_candidates(
                        &state.sql_modal.content,
                        cursor,
                        state.cache.metadata.as_ref(),
                        state.cache.table_detail.as_ref(),
                        &recent_cols,
                    );
                    let visible =
                        !candidates.is_empty() && !state.sql_modal.content.trim().is_empty();
                    (candidates, token_len, visible)
                };

                let _ = self
                    .action_tx
                    .send(Action::CompletionUpdated {
                        candidates,
                        trigger_position: cursor.saturating_sub(token_len),
                        visible,
                    })
                    .await;
                Ok(())
            }

            Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name,
                target_tables,
            } => {
                use crate::domain::er::{er_output_filename, fk_reachable_tables_multi};

                let all_tables: Vec<ErTableInfo> = {
                    let engine = completion_engine.borrow();
                    engine
                        .table_details_iter()
                        .map(|(k, v)| ErTableInfo::from_table(k, v))
                        .collect()
                };

                if all_tables.is_empty() {
                    let _ = self
                        .action_tx
                        .send(Action::ErDiagramFailed(
                            "No table data loaded yet".to_string(),
                        ))
                        .await;
                    return Ok(());
                }

                let total = all_tables.len();
                let filename = er_output_filename(&target_tables, total);
                let tables = if target_tables.is_empty() || target_tables.len() == total {
                    all_tables
                } else {
                    fk_reachable_tables_multi(&all_tables, &target_tables, 1)
                };

                if tables.is_empty() {
                    let _ = self
                        .action_tx
                        .send(Action::ErDiagramFailed(
                            "Selected tables not found in cached data".to_string(),
                        ))
                        .await;
                    return Ok(());
                }

                let cache_dir = self.config_writer.get_cache_dir(&project_name)?;
                let exporter = Arc::clone(&self.er_exporter);
                spawn_er_diagram_task(
                    exporter,
                    tables,
                    total_tables,
                    cache_dir,
                    self.action_tx.clone(),
                    filename,
                );
                Ok(())
            }

            Effect::WriteErFailureLog { failed_tables } => {
                if let Ok(cache_dir) = self
                    .config_writer
                    .get_cache_dir(&state.runtime.project_name)
                {
                    let writer = Arc::clone(&self.er_log_writer);
                    tokio::task::spawn_blocking(move || {
                        // Log write failure is intentionally ignored: the app has no
                        // output channel (TUI is active) and tracing is not available.
                        let _ = writer.write_er_failure_log(failed_tables, cache_dir);
                    });
                }
                Ok(())
            }

            Effect::Sequence(_) => {
                // Handled in run()
                Ok(())
            }

            Effect::DispatchActions(actions) => {
                for action in actions {
                    let _ = self.action_tx.send(action).await;
                }
                Ok(())
            }

            Effect::CopyToClipboard {
                content,
                on_success,
                on_failure,
            } => {
                let tx = self.action_tx.clone();
                tokio::task::spawn_blocking(move || {
                    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&content)) {
                        Ok(()) => {
                            if let Some(action) = on_success {
                                let _ = tx.blocking_send(action);
                            }
                        }
                        Err(e) => {
                            if let Some(action) = on_failure {
                                let _ = tx.blocking_send(action);
                            } else {
                                let _ = tx.blocking_send(Action::CopyFailed(e.to_string()));
                            }
                        }
                    }
                });
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::completion::CompletionEngine;
    use crate::app::ports::connection_store::MockConnectionStore;
    use crate::app::ports::metadata::MockMetadataProvider;
    use crate::app::ports::query_executor::MockQueryExecutor;
    use crate::app::ports::{
        ConfigWriter, ErDiagramExporter, ErExportResult, ErLogWriter, RenderOutput, Renderer,
    };
    use crate::domain::connection::ConnectionId;
    use crate::domain::{DatabaseMetadata, QueryResult, QuerySource};
    use color_eyre::eyre::Result;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use tokio::sync::mpsc;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(&mut self, _state: &mut AppState) -> Result<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    struct NoopConfigWriter;
    impl ConfigWriter for NoopConfigWriter {
        fn get_cache_dir(&self, _project_name: &str) -> Result<PathBuf> {
            Ok(PathBuf::from("/tmp"))
        }
    }

    struct NoopErExporter;
    impl ErDiagramExporter for NoopErExporter {
        fn generate_and_export(
            &self,
            _tables: &[crate::domain::ErTableInfo],
            _filename: &str,
            _cache_dir: &Path,
        ) -> ErExportResult<PathBuf> {
            Ok(PathBuf::from("/tmp/er.svg"))
        }
    }

    struct NoopErLogWriter;
    impl ErLogWriter for NoopErLogWriter {
        fn write_er_failure_log(
            &self,
            _failed_tables: Vec<(String, String)>,
            _cache_dir: PathBuf,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn make_runner(
        metadata_provider: Arc<dyn MetadataProvider>,
        query_executor: Arc<dyn QueryExecutor>,
        connection_store: Arc<dyn ConnectionStore>,
        cache: TtlCache<String, DatabaseMetadata>,
        action_tx: mpsc::Sender<Action>,
    ) -> EffectRunner {
        EffectRunner::new(
            metadata_provider,
            query_executor,
            Arc::new(NoopErExporter),
            Arc::new(NoopConfigWriter),
            Arc::new(NoopErLogWriter),
            connection_store,
            cache,
            action_tx,
        )
    }

    fn sample_metadata() -> DatabaseMetadata {
        DatabaseMetadata {
            database_name: "testdb".to_string(),
            schemas: vec![],
            tables: vec![],
            fetched_at: Instant::now(),
        }
    }

    fn sample_query_result() -> QueryResult {
        QueryResult {
            query: "SELECT 1".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            row_count: 1,
            execution_time_ms: 5,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
        }
    }

    #[tokio::test]
    async fn fetch_metadata_cache_hit_skips_provider() {
        let mut mock_provider = MockMetadataProvider::new();
        mock_provider.expect_fetch_metadata().never();

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
        cache.set("dsn://test".to_string(), sample_metadata()).await;

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
    async fn fetch_metadata_cache_miss_calls_provider_ok() {
        let mut mock_provider = MockMetadataProvider::new();
        mock_provider
            .expect_fetch_metadata()
            .once()
            .returning(|_| Ok(sample_metadata()));

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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
    async fn fetch_metadata_provider_error_sends_failed() {
        let mut mock_provider = MockMetadataProvider::new();
        mock_provider.expect_fetch_metadata().once().returning(|_| {
            Err(crate::app::ports::MetadataError::ConnectionFailed(
                "timeout".to_string(),
            ))
        });

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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

    #[tokio::test]
    async fn execute_preview_ok_sends_query_completed() {
        let mut mock_executor = MockQueryExecutor::new();
        mock_executor
            .expect_execute_preview()
            .once()
            .returning(|_, _, _, _, _| Ok(sample_query_result()));

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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
                }],
                &mut renderer,
                state,
                &ce,
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
    async fn execute_preview_error_sends_query_failed() {
        let mut mock_executor = MockQueryExecutor::new();
        mock_executor
            .expect_execute_preview()
            .once()
            .returning(|_, _, _, _, _| {
                Err(crate::app::ports::MetadataError::QueryFailed(
                    "syntax error".to_string(),
                ))
            });

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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
                }],
                &mut renderer,
                state,
                &ce,
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

    #[tokio::test]
    async fn delete_connection_ok_sends_deleted() {
        let mut mock_store = MockConnectionStore::new();
        mock_store.expect_delete().once().returning(|_| Ok(()));

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
        let (tx, mut rx) = mpsc::channel(8);
        let runner = make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(MockQueryExecutor::new()),
            Arc::new(mock_store),
            cache,
            tx,
        );

        let id = ConnectionId::new();
        let state = &mut AppState::new("test".to_string());
        let ce = RefCell::new(CompletionEngine::new());
        let mut renderer = NoopRenderer;

        runner
            .run(
                vec![Effect::DeleteConnection { id: id.clone() }],
                &mut renderer,
                state,
                &ce,
            )
            .await
            .unwrap();

        let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .expect("action timeout")
            .expect("channel closed");
        assert!(
            matches!(action, Action::ConnectionDeleted(_)),
            "expected ConnectionDeleted, got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn delete_connection_error_sends_failed() {
        let mut mock_store = MockConnectionStore::new();
        mock_store.expect_delete().once().returning(|_| {
            Err(crate::app::ports::ConnectionStoreError::NotFound(
                "id".to_string(),
            ))
        });

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
        let (tx, mut rx) = mpsc::channel(8);
        let runner = make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(MockQueryExecutor::new()),
            Arc::new(mock_store),
            cache,
            tx,
        );

        let id = ConnectionId::new();
        let state = &mut AppState::new("test".to_string());
        let ce = RefCell::new(CompletionEngine::new());
        let mut renderer = NoopRenderer;

        runner
            .run(
                vec![Effect::DeleteConnection { id }],
                &mut renderer,
                state,
                &ce,
            )
            .await
            .unwrap();

        let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .expect("action timeout")
            .expect("channel closed");
        assert!(
            matches!(action, Action::ConnectionDeleteFailed(_)),
            "expected ConnectionDeleteFailed, got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn load_connections_error_sends_empty_list() {
        let mut mock_store = MockConnectionStore::new();
        mock_store.expect_load_all().once().returning(|| {
            Err(crate::app::ports::ConnectionStoreError::ReadError(
                "file not found".to_string(),
            ))
        });

        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
        let (tx, mut rx) = mpsc::channel(8);
        let runner = make_runner(
            Arc::new(MockMetadataProvider::new()),
            Arc::new(MockQueryExecutor::new()),
            Arc::new(mock_store),
            cache,
            tx,
        );

        let state = &mut AppState::new("test".to_string());
        let ce = RefCell::new(CompletionEngine::new());
        let mut renderer = NoopRenderer;

        runner
            .run(vec![Effect::LoadConnections], &mut renderer, state, &ce)
            .await
            .unwrap();

        let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .expect("action timeout")
            .expect("channel closed");
        assert!(
            matches!(action, Action::ConnectionsLoaded(ref v) if v.is_empty()),
            "expected ConnectionsLoaded([]), got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn cache_invalidate_removes_entry() {
        let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
        cache
            .set("dsn://target".to_string(), sample_metadata())
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
            )
            .await
            .unwrap();

        assert!(cache.get(&"dsn://target".to_string()).await.is_none());
    }
}
