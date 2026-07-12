// RefCell Borrow Safety: when effects need data from `completion_engine`,
// the borrow MUST be dropped before any await point.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::browse as cmd_browse;
use crate::cmd::cache::TtlCache;
use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::connection as cmd_connection;
use crate::cmd::effect::Effect;
use crate::cmd::er::handler as cmd_er;
use crate::cmd::query_task::QueryTaskRegistry;
use crate::cmd::settings as cmd_settings;
use crate::cmd::sql_editor::completion as cmd_completion;
use crate::cmd::sql_editor::query_history as cmd_query_history;
use crate::cmd::sqlite_diagnostics;
use crate::cmd::utility as cmd_utility;
use crate::domain::DatabaseMetadata;
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    CachedResultExporter, ClipboardWriter, ConfigWriter, ConnectionStore, DsnBuilder,
    ErDiagramExporter, ErLogWriter, FolderOpener, MetadataProvider, PgServiceEntryReader,
    QueryExecutor, QueryHistoryStore, Renderer, SettingsStore, SqliteDiagnosticsProvider,
    SqlitePathValidator,
};
use crate::services::AppServices;
use crate::update::action::Action;

pub struct ConnectionDeps {
    pub dsn_builder: Arc<dyn DsnBuilder>,
    pub connection_store: Arc<dyn ConnectionStore>,
    pub pg_service_entry_reader: Option<Arc<dyn PgServiceEntryReader>>,
    pub sqlite_path_validator: Arc<dyn SqlitePathValidator>,
}

pub struct QueryDeps {
    pub query_executor: Arc<dyn QueryExecutor>,
    pub query_history_store: Arc<dyn QueryHistoryStore>,
    pub sqlite_diagnostics: Arc<dyn SqliteDiagnosticsProvider>,
    pub cached_result_exporter: Arc<dyn CachedResultExporter>,
}

pub struct ErDeps {
    pub er_exporter: Arc<dyn ErDiagramExporter>,
    pub config_writer: Arc<dyn ConfigWriter>,
    pub er_log_writer: Arc<dyn ErLogWriter>,
}

pub struct UtilityDeps {
    pub clipboard: Arc<dyn ClipboardWriter>,
    pub folder_opener: Arc<dyn FolderOpener>,
}

pub struct SettingsDeps {
    pub settings_store: Arc<dyn SettingsStore>,
}

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    connection: ConnectionDeps,
    query: QueryDeps,
    er: ErDeps,
    utility: UtilityDeps,
    settings: SettingsDeps,
    metadata_cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    query_tasks: QueryTaskRegistry,
}

impl EffectRunner {
    pub fn new(
        metadata_provider: Arc<dyn MetadataProvider>,
        connection: ConnectionDeps,
        query: QueryDeps,
        er: ErDeps,
        utility: UtilityDeps,
        settings: SettingsDeps,
        metadata_cache: TtlCache<String, Arc<DatabaseMetadata>>,
        action_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            metadata_provider,
            connection,
            query,
            er,
            utility,
            settings,
            metadata_cache,
            action_tx,
            query_tasks: QueryTaskRegistry::default(),
        }
    }

    pub fn action_tx(&self) -> &mpsc::Sender<Action> {
        &self.action_tx
    }

    pub async fn run<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        services: &AppServices,
    ) -> Result<Vec<Action>> {
        let mut dispatched = Vec::new();
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        dispatched.extend(
                            self.run_normal(seq_effect, tui, state, completion_engine, services)
                                .await?,
                        );
                    }
                }
                single_effect => {
                    dispatched.extend(
                        self.run_normal(single_effect, tui, state, completion_engine, services)
                            .await?,
                    );
                }
            }
        }
        Ok(dispatched)
    }

    async fn run_normal<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        services: &AppServices,
    ) -> Result<Vec<Action>> {
        match effect {
            Effect::Render => {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "the effect runner is the runtime boundary that reads the clock for rendering"
                )]
                let now = Instant::now();
                let output = tui.draw(state, services, now)?;
                state.apply_render_output(output);
                Ok(vec![])
            }

            Effect::Sequence(_) => {
                // Handled in run()
                Ok(vec![])
            }
            Effect::DispatchActions(actions) => Ok(actions),

            e @ (Effect::CopyToClipboard { .. } | Effect::OpenFolder { .. }) => {
                cmd_utility::run(
                    e,
                    &self.action_tx,
                    &self.utility.clipboard,
                    &self.utility.folder_opener,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::SaveAndConnect { .. }
            | Effect::LoadConnectionForEdit { .. }
            | Effect::LoadConnections
            | Effect::DeleteConnection { .. }
            | Effect::SwitchConnection { .. }
            | Effect::SwitchToService { .. }) => {
                cmd_connection::run(
                    e,
                    &self.action_tx,
                    &self.connection,
                    &self.metadata_provider,
                    &self.metadata_cache,
                    state,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::FetchMetadata { .. }
            | Effect::FetchTableDetail { .. }
            | Effect::PrefetchTableDetail { .. }
            | Effect::ProcessPrefetchQueue { .. }
            | Effect::DelayedProcessPrefetchQueue { .. }
            | Effect::CacheInvalidate { .. }) => {
                cmd_browse::metadata::run(
                    e,
                    &self.action_tx,
                    &self.metadata_provider,
                    &self.metadata_cache,
                    &self.connection.sqlite_path_validator,
                    state,
                    completion_engine,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::ExecutePreview { .. }
            | Effect::ExecuteAdhoc { .. }
            | Effect::ExecuteExplain { .. }
            | Effect::ExecuteWrite { .. }
            | Effect::CancelActiveQuery
            | Effect::CountRowsForExport { .. }
            | Effect::ExportCsv { .. }
            | Effect::ExportCsvFromCache { .. }) => {
                cmd_browse::query::run(
                    e,
                    &self.action_tx,
                    &self.query.query_executor,
                    &self.query.query_history_store,
                    &self.query.cached_result_exporter,
                    &self.query_tasks,
                    state,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::GenerateErDiagramFromCache { .. }
            | Effect::ExtractFkNeighbors { .. }
            | Effect::WriteErFailureLog { .. }
            | Effect::SmartErRefresh { .. }) => {
                cmd_er::run(
                    e,
                    &self.action_tx,
                    &self.metadata_provider,
                    &self.er.er_exporter,
                    &self.er.config_writer,
                    &self.er.er_log_writer,
                    state,
                    completion_engine,
                )
                .await?;
                Ok(vec![])
            }

            e @ Effect::LoadQueryHistory { .. } => {
                cmd_query_history::run(e, &self.action_tx, &self.query.query_history_store);
                Ok(vec![])
            }

            e @ Effect::SaveSettings { .. } => {
                cmd_settings::run(e, &self.action_tx, &self.settings.settings_store).await;
                Ok(vec![])
            }

            e @ (Effect::FetchSqliteDiagnosticsCore { .. }
            | Effect::FetchSqliteDiagnosticsQuickCheck { .. }) => {
                sqlite_diagnostics::run(e, &self.action_tx, &self.query.sqlite_diagnostics);
                Ok(vec![])
            }

            e @ (Effect::CacheTableInCompletionEngine { .. }
            | Effect::EvictTablesFromCompletionCache { .. }
            | Effect::ClearCompletionEngineCache
            | Effect::ResizeCompletionCache { .. }
            | Effect::TriggerCompletion) => {
                cmd_completion::run(e, &self.action_tx, state, completion_engine).await?;
                Ok(vec![])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::test_fixtures;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{RenderOutput, RenderResult};
    use crate::services::AppServices;
    use tokio::sync::mpsc;

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

    mod render {
        use super::*;
        use crate::model::browse::jsonb_detail::JsonbDetailState;

        struct ExplorerWidthRenderer {
            explorer_content_width: usize,
        }

        struct JsonbVisibleRowsRenderer {
            visible_rows: usize,
        }

        impl Renderer for ExplorerWidthRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput {
                    explorer_content_width: self.explorer_content_width,
                    ..RenderOutput::default()
                })
            }
        }

        impl Renderer for JsonbVisibleRowsRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput {
                    jsonb_detail_editor_visible_rows: Some(self.visible_rows),
                    ..RenderOutput::default()
                })
            }
        }

        #[tokio::test]
        async fn calls_draw() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
        }

        #[tokio::test]
        async fn clamps_stale_explorer_horizontal_offset_to_new_maximum() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            state.session.set_metadata(Some(Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = vec![TableSummary::new(
                    "public".to_string(),
                    "abcdefghij".to_string(),
                    Some(0),
                    false,
                )];
                metadata
            })));
            state.ui.set_explorer_horizontal_offset(20);

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = ExplorerWidthRenderer {
                explorer_content_width: 8,
            };

            runner
                .run(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(state.ui.explorer_horizontal_offset(), 9);
        }

        #[tokio::test]
        async fn recomputes_jsonb_editor_scroll_when_visible_rows_change() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            state.jsonb_detail = JsonbDetailState::open_pretty(
                0,
                0,
                "settings".to_string(),
                "{}".to_string(),
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
            );
            state.jsonb_detail.editor_mut().set_content_with_cursor(
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
                29,
            );

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = JsonbVisibleRowsRenderer { visible_rows: 2 };

            runner
                .run(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(state.ui.jsonb_detail_editor_visible_rows(), 2);
            assert_eq!(state.jsonb_detail.editor().cursor_to_position().0, 3);
            assert_eq!(state.jsonb_detail.editor().scroll_row(), 2);
        }
    }

    mod dispatch_actions {
        use super::*;

        #[tokio::test]
        async fn dispatches_all_actions() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            let result = runner
                .run(
                    vec![Effect::DispatchActions(vec![
                        Action::ProcessPrefetchQueue { run_id: 1 },
                        Action::ProcessPrefetchQueue { run_id: 1 },
                    ])],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(result.len(), 2);
            assert!(matches!(
                result[0],
                Action::ProcessPrefetchQueue { run_id: 1 }
            ));
            assert!(matches!(
                result[1],
                Action::ProcessPrefetchQueue { run_id: 1 }
            ));
        }
    }

    mod query_context_termination {
        use std::future::pending;
        use std::path::Path;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};

        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::connection::{ConnectionId, DatabaseType};
        use crate::domain::{QueryResult, WriteExecutionResult};
        use crate::model::connection::cache::ConnectionCache;
        use crate::ports::outbound::{AccessMode, DbOperationError};
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;

        struct DropSignal(Arc<AtomicBool>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        struct PendingQueryExecutor {
            started: Mutex<Option<oneshot::Sender<()>>>,
            dropped: Arc<AtomicBool>,
        }

        #[async_trait::async_trait]
        impl QueryExecutor for PendingQueryExecutor {
            async fn execute_preview(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
                _limit: usize,
                _offset: usize,
            ) -> Result<QueryResult, DbOperationError> {
                let _guard = DropSignal(Arc::clone(&self.dropped));
                self.started
                    .lock()
                    .expect("started signal lock poisoned")
                    .take()
                    .expect("preview should start once")
                    .send(())
                    .ok();
                pending().await
            }

            async fn execute_adhoc(
                &self,
                _dsn: &str,
                _query: &str,
                _access_mode: AccessMode,
            ) -> Result<QueryResult, DbOperationError> {
                unreachable!("test only starts a preview")
            }

            async fn execute_write(
                &self,
                _dsn: &str,
                _query: &str,
                _access_mode: AccessMode,
            ) -> Result<WriteExecutionResult, DbOperationError> {
                unreachable!("test only starts a preview")
            }

            async fn count_query_rows(
                &self,
                _dsn: &str,
                _query: &str,
            ) -> Result<usize, DbOperationError> {
                unreachable!("test only starts a preview")
            }

            async fn export_to_csv(
                &self,
                _dsn: &str,
                _query: &str,
                _path: &Path,
            ) -> Result<usize, DbOperationError> {
                unreachable!("test only starts a preview")
            }
        }

        #[tokio::test]
        async fn connection_switch_drops_pending_query_task() {
            let (started_tx, started_rx) = oneshot::channel();
            let dropped = Arc::new(AtomicBool::new(false));
            let executor = PendingQueryExecutor {
                started: Mutex::new(Some(started_tx)),
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(executor),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                action_tx,
            );
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
            );
            let target_id = ConnectionId::new();
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());
            let run_id = state.query.begin_running(Instant::now());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExecutePreview {
                        dsn: "postgres://localhost/current".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        run_id,
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            timeout(Duration::from_secs(1), started_rx)
                .await
                .expect("pending query should start")
                .expect("started signal should be sent");

            let effects = reduce(
                &mut state,
                Action::SwitchConnection(ConnectionTarget {
                    id: target_id,
                    dsn: "sqlite:///tmp/target.db".to_string(),
                    name: "target".to_string(),
                    database_type: DatabaseType::SQLite,
                }),
                Instant::now(),
                &AppServices::stub(),
            );
            runner
                .run(
                    effects,
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            timeout(Duration::from_secs(1), async {
                while !dropped.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("context termination should drop the pending query task");
            assert!(!state.query.is_current_run(run_id));
        }
    }
}
