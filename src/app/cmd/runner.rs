// RefCell Borrow Safety: when effects need data from `completion_engine`,
// the borrow MUST be dropped before any await point.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::browse as cmd_browse;
use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::connection as cmd_connection;
use crate::cmd::connection::ConnectionTaskOwner;
use crate::cmd::effect::Effect;
use crate::cmd::er::handler as cmd_er;
use crate::cmd::er::task::SmartErRefreshTaskOwner;
use crate::cmd::metadata_task::MetadataTaskRegistry;
use crate::cmd::settings as cmd_settings;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::cmd::sql_editor::completion as cmd_completion;
use crate::cmd::sql_editor::query_history::spawn_query_history_load;
use crate::cmd::sqlite_diagnostics;
use crate::cmd::utility as cmd_utility;
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    CachedResultExporter, ClipboardWriter, ConfigWriter, ConnectionStore, DsnBuilder,
    ErDiagramExporter, ErLogWriter, FolderOpener, MetadataProvider, MySqlConnectionProbe,
    PgServiceEntryReader, QueryExecutor, QueryHistoryStore, Renderer, SettingsStore,
    SqliteDiagnosticsProvider, SqlitePathValidator,
};
use crate::services::AppServices;
use crate::update::action::Action;

pub struct ConnectionDeps {
    pub dsn_builder: Arc<dyn DsnBuilder>,
    pub mysql_connection_probe: Arc<dyn MySqlConnectionProbe>,
    pub connection_store: Arc<dyn ConnectionStore>,
    pub pg_service_entry_reader: Arc<dyn PgServiceEntryReader>,
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

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    connection: ConnectionDeps,
    query: QueryDeps,
    er: ErDeps,
    utility: UtilityDeps,
    settings_store: Arc<dyn SettingsStore>,
    action_tx: mpsc::Sender<Action>,
    query_tasks: SingleTaskOwner,
    table_detail_tasks: SingleTaskOwner,
    metadata_tasks: Arc<MetadataTaskRegistry>,
    connection_task: ConnectionTaskOwner,
    sqlite_diagnostics_task: sqlite_diagnostics::SqliteDiagnosticsTaskOwner,
    smart_er_refresh_task: SmartErRefreshTaskOwner,
}

impl EffectRunner {
    pub fn new(
        metadata_provider: Arc<dyn MetadataProvider>,
        connection: ConnectionDeps,
        query: QueryDeps,
        er: ErDeps,
        utility: UtilityDeps,
        settings_store: Arc<dyn SettingsStore>,
        action_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            metadata_provider,
            connection,
            query,
            er,
            utility,
            settings_store,
            action_tx,
            query_tasks: SingleTaskOwner::default(),
            table_detail_tasks: SingleTaskOwner::default(),
            metadata_tasks: Arc::new(MetadataTaskRegistry::default()),
            connection_task: ConnectionTaskOwner::default(),
            sqlite_diagnostics_task: sqlite_diagnostics::SqliteDiagnosticsTaskOwner::default(),
            smart_er_refresh_task: SmartErRefreshTaskOwner::default(),
        }
    }

    pub fn action_tx(&self) -> &mpsc::Sender<Action> {
        &self.action_tx
    }

    async fn cancel_tracked_tasks(&self) {
        let connection_task = self.connection_task.abort();
        let metadata_task = self.metadata_tasks.abort();
        let smart_er_task = self.smart_er_refresh_task.abort();
        let sqlite_diagnostics_tasks = self.sqlite_diagnostics_task.abort();
        let query_task = self.query_tasks.abort();
        let table_detail_task = self.table_detail_tasks.abort();
        if let Some(task) = metadata_task {
            let _ = task.await;
        }
        if let Some(task) = query_task {
            let _ = task.await;
        }
        if let Some(task) = table_detail_task {
            let _ = task.await;
        }
        if let Some(task) = connection_task {
            let _ = task.await;
        }
        for task in smart_er_task.into_iter().chain(sqlite_diagnostics_tasks) {
            let _ = task.await;
        }
    }

    pub async fn execute_effects<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        services: &AppServices,
    ) -> Result<Vec<Action>> {
        let mut dispatched = Vec::new();
        for effect in effects {
            dispatched.extend(
                self.execute_single_effect(effect, tui, state, completion_engine, services)
                    .await?,
            );
        }
        Ok(dispatched)
    }

    async fn execute_single_effect<T: Renderer>(
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

            Effect::DispatchActions(actions) => Ok(actions),

            e @ (Effect::CopyToClipboard { .. } | Effect::OpenFolder { .. }) => {
                cmd_utility::run(
                    e,
                    &self.action_tx,
                    &self.utility.clipboard,
                    &self.utility.folder_opener,
                )
                .await;
                Ok(vec![])
            }

            e @ (Effect::SaveAndConnect { .. }
            | Effect::ProbeMySqlConnection { .. }
            | Effect::LoadConnectionForEdit { .. }
            | Effect::LoadConnections
            | Effect::DeleteConnection { .. }
            | Effect::SwitchConnection { .. }
            | Effect::SwitchToService { .. }) => {
                if matches!(
                    &e,
                    Effect::SaveAndConnect { .. }
                        | Effect::ProbeMySqlConnection { .. }
                        | Effect::SwitchConnection { .. }
                        | Effect::SwitchToService { .. }
                ) {
                    self.metadata_tasks.cancel().await;
                }
                if matches!(
                    &e,
                    Effect::SwitchConnection { .. } | Effect::SwitchToService { .. }
                ) {
                    self.smart_er_refresh_task.cancel().await;
                }
                if matches!(&e, Effect::ProbeMySqlConnection { .. }) {
                    self.table_detail_tasks.cancel().await;
                }
                cmd_connection::run(
                    e,
                    &self.action_tx,
                    &self.connection,
                    &self.connection_task,
                    &self.metadata_provider,
                    state,
                )
                .await;
                Ok(vec![])
            }

            e @ (Effect::FetchMetadata { .. }
            | Effect::FetchEffectiveUser { .. }
            | Effect::FetchTableDetail { .. }
            | Effect::PrefetchTableColumnsAndFks { .. }
            | Effect::SchedulePrefetchQueueProcessing { .. }
            | Effect::DelayedProcessPrefetchQueue { .. }
            | Effect::CancelMetadataTasks) => {
                if matches!(&e, Effect::FetchMetadata { .. }) {
                    self.metadata_tasks.cancel().await;
                    self.smart_er_refresh_task.cancel().await;
                }
                cmd_browse::metadata::run(
                    e,
                    &self.action_tx,
                    &self.metadata_provider,
                    &self.connection.sqlite_path_validator,
                    &self.table_detail_tasks,
                    &self.metadata_tasks,
                    completion_engine,
                )
                .await;
                Ok(vec![])
            }

            Effect::CancelConnectionTask => {
                self.connection_task.cancel().await;
                Ok(vec![])
            }

            Effect::CancelSqliteDiagnostics => {
                self.sqlite_diagnostics_task.cancel().await;
                Ok(vec![])
            }

            Effect::CancelTrackedTasks => {
                self.cancel_tracked_tasks().await;
                Ok(vec![])
            }

            e @ (Effect::ExecutePreview { .. }
            | Effect::ExecuteAdhoc { .. }
            | Effect::ExecuteExplain { .. }
            | Effect::ExecuteWrite { .. }
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
                .await;
                Ok(vec![])
            }

            e @ (Effect::GenerateErDiagramFromCache { .. }
            | Effect::ExtractFkNeighbors { .. }
            | Effect::WriteErFailureLog { .. }
            | Effect::SmartErRefreshCacheAndDiff { .. }) => {
                cmd_er::run(
                    e,
                    &self.action_tx,
                    &self.er.er_exporter,
                    &self.er.config_writer,
                    &self.er.er_log_writer,
                    state,
                    completion_engine,
                )
                .await?;
                Ok(vec![])
            }

            Effect::SmartErRefresh { dsn, run_id } => {
                self.smart_er_refresh_task
                    .replace(cmd_er::smart_refresh_task(
                        self.action_tx.clone(),
                        Arc::clone(&self.metadata_provider),
                        dsn,
                        run_id,
                    ))
                    .await;
                Ok(vec![])
            }

            Effect::LoadQueryHistory {
                project_name,
                scope,
            } => {
                spawn_query_history_load(
                    project_name,
                    scope,
                    &self.action_tx,
                    &self.query.query_history_store,
                );
                Ok(vec![])
            }

            Effect::SaveSettings { settings } => {
                cmd_settings::run(settings, &self.action_tx, &self.settings_store).await;
                Ok(vec![])
            }

            e @ (Effect::FetchSqliteDiagnosticsCore { .. }
            | Effect::FetchSqliteDiagnosticsQuickCheck { .. }) => {
                sqlite_diagnostics::run(
                    e,
                    &self.action_tx,
                    &self.query.sqlite_diagnostics,
                    &self.sqlite_diagnostics_task,
                )
                .await;
                Ok(vec![])
            }

            e @ (Effect::CacheTableInCompletionEngine { .. }
            | Effect::EvictTablesFromCompletionCache { .. }
            | Effect::ClearCompletionEngineCache
            | Effect::ResizeCompletionCache { .. }
            | Effect::TriggerCompletion) => Ok(cmd_completion::run(e, state, completion_engine)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::test_fixtures::{self, NoopRenderer};
    use crate::domain::{DatabaseMetadata, TableSignatureSnapshot, TableSummary};
    use crate::model::shared::render_output::{
        BrowseLayout, DetailLayout, ExplorerLayout, JsonDetailLayout,
    };
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{MySqlConnectionProbeResult, RenderOutput, RenderResult};
    use crate::services::AppServices;
    use tokio::sync::mpsc;

    mod render {
        use super::*;
        use crate::model::browse::json_detail::JsonDetailState;

        struct ExplorerWidthRenderer {
            explorer_content_width: usize,
        }

        struct JsonVisibleRowsRenderer {
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
                    browse: BrowseLayout {
                        explorer: ExplorerLayout {
                            content_width: self.explorer_content_width,
                            ..ExplorerLayout::default()
                        },
                        ..BrowseLayout::default()
                    },
                    ..RenderOutput::default()
                })
            }
        }

        impl Renderer for JsonVisibleRowsRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> RenderResult<RenderOutput> {
                Ok(RenderOutput {
                    details: DetailLayout {
                        json: Some(JsonDetailLayout {
                            editor_visible_rows: self.visible_rows,
                        }),
                        ..DetailLayout::default()
                    },
                    ..RenderOutput::default()
                })
            }
        }

        #[tokio::test]
        async fn calls_draw() {
            let (tx, mut _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            test_fixtures::run_one_effect(
                &runner,
                Effect::Render,
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut _rx,
                None,
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
                .execute_effects(
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
        async fn recomputes_json_editor_scroll_when_visible_rows_change() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            state.json_detail = JsonDetailState::open_pretty(
                0,
                0,
                "settings".to_string(),
                "{}".to_string(),
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
            );
            state.json_detail.editor_mut().set_content_with_cursor(
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
                29,
            );

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = JsonVisibleRowsRenderer { visible_rows: 2 };

            runner
                .execute_effects(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(state.ui.json_detail_editor_visible_rows(), 2);
            assert_eq!(state.json_detail.editor().cursor_to_position().0, 3);
            assert_eq!(state.json_detail.editor().scroll_row(), 2);
        }
    }

    mod task_cancellation {
        use std::future::pending;
        use std::sync::{Condvar, Mutex};
        use std::time::Duration as StdDuration;

        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};

        use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
        use crate::ports::outbound::DbOperationError;

        use super::*;

        struct BlockingDropState {
            entered: Mutex<Option<oneshot::Sender<()>>>,
            released: (Mutex<bool>, Condvar),
            finished: Mutex<Option<oneshot::Sender<()>>>,
        }

        impl BlockingDropState {
            fn release(&self) {
                let (released, condvar) = &self.released;
                *released.lock().expect("release lock poisoned") = true;
                condvar.notify_one();
            }
        }

        struct BlockingDrop(Arc<BlockingDropState>);

        impl Drop for BlockingDrop {
            fn drop(&mut self) {
                self.0
                    .entered
                    .lock()
                    .expect("drop signal lock poisoned")
                    .take()
                    .expect("drop signal should be observed once")
                    .send(())
                    .ok();
                let (released, condvar) = &self.0.released;
                let mut released = released.lock().expect("release lock poisoned");
                while !*released {
                    let (next, result) = condvar
                        .wait_timeout(released, StdDuration::from_secs(10))
                        .expect("release wait poisoned");
                    released = next;
                    if result.timed_out() {
                        break;
                    }
                }
                let finished_sender = self
                    .0
                    .finished
                    .lock()
                    .expect("finished signal lock poisoned")
                    .take();
                if let Some(sender) = finished_sender {
                    sender.send(()).ok();
                }
            }
        }

        struct TaskDropSignal(Mutex<Option<oneshot::Sender<()>>>);

        impl Drop for TaskDropSignal {
            fn drop(&mut self) {
                self.0
                    .lock()
                    .expect("task drop signal lock poisoned")
                    .take()
                    .expect("task drop signal should be observed once")
                    .send(())
                    .ok();
            }
        }

        struct BlockingDiagnosticsProvider {
            started: Arc<tokio::sync::Notify>,
            drop_state: Arc<BlockingDropState>,
        }

        #[async_trait::async_trait]
        impl SqliteDiagnosticsProvider for BlockingDiagnosticsProvider {
            async fn fetch_core_diagnostics(
                &self,
                _dsn: &str,
            ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
                let _drop_signal = BlockingDrop(Arc::clone(&self.drop_state));
                self.started.notify_one();
                pending().await
            }

            async fn fetch_quick_check(&self, _dsn: &str) -> DiagnosticField {
                let _drop_signal = BlockingDrop(Arc::clone(&self.drop_state));
                self.started.notify_one();
                pending().await
            }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn cancel_tracked_tasks_aborts_all_groups_before_joining() {
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let (query_started_tx, query_started_rx) = oneshot::channel();
            let (query_drop_tx, mut query_drop_rx) = oneshot::channel();
            let (query_finished_tx, mut query_finished_rx) = oneshot::channel();
            let query_drop_state = Arc::new(BlockingDropState {
                entered: Mutex::new(Some(query_drop_tx)),
                released: (Mutex::new(false), Condvar::new()),
                finished: Mutex::new(Some(query_finished_tx)),
            });
            runner
                .query_tasks
                .replace({
                    let drop_state = Arc::clone(&query_drop_state);
                    async move {
                        let _drop_signal = BlockingDrop(drop_state);
                        query_started_tx.send(()).ok();
                        pending::<()>().await;
                    }
                })
                .await;
            query_started_rx.await.expect("query task should start");

            let (table_started_tx, table_started_rx) = oneshot::channel();
            let (table_drop_tx, mut table_drop_rx) = oneshot::channel();
            runner
                .table_detail_tasks
                .replace({
                    async move {
                        let _drop_signal = TaskDropSignal(Mutex::new(Some(table_drop_tx)));
                        table_started_tx.send(()).ok();
                        pending::<()>().await;
                    }
                })
                .await;
            table_started_rx
                .await
                .expect("table detail task should start");

            let (connection_started_tx, connection_started_rx) = oneshot::channel();
            let (connection_drop_tx, mut connection_drop_rx) = oneshot::channel();
            runner
                .connection_task
                .replace(async move {
                    let _drop_signal = TaskDropSignal(Mutex::new(Some(connection_drop_tx)));
                    connection_started_tx.send(()).ok();
                    pending::<()>().await;
                })
                .await;
            connection_started_rx
                .await
                .expect("connection task should start");

            let diagnostics_started = Arc::new(tokio::sync::Notify::new());
            let (diagnostics_drop_tx, mut diagnostics_drop_rx) = oneshot::channel();
            let diagnostics_drop_state = Arc::new(BlockingDropState {
                entered: Mutex::new(Some(diagnostics_drop_tx)),
                released: (Mutex::new(false), Condvar::new()),
                finished: Mutex::new(None),
            });
            let diagnostics_provider = Arc::new(BlockingDiagnosticsProvider {
                started: Arc::clone(&diagnostics_started),
                drop_state: Arc::clone(&diagnostics_drop_state),
            }) as Arc<dyn SqliteDiagnosticsProvider>;
            sqlite_diagnostics::run(
                Effect::FetchSqliteDiagnosticsCore {
                    dsn: "sqlite:///tmp/app.db".to_string(),
                    run_id: 1,
                },
                &runner.action_tx,
                &diagnostics_provider,
                &runner.sqlite_diagnostics_task,
            )
            .await;
            timeout(Duration::from_secs(1), diagnostics_started.notified())
                .await
                .expect("SQLite diagnostics task should start");

            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = test_fixtures::NoopRenderer;
            let services = AppServices::stub();
            let mut follow_up = Box::pin(runner.execute_effects(
                vec![
                    Effect::CancelTrackedTasks,
                    Effect::DispatchActions(vec![Action::Render]),
                ],
                &mut renderer,
                &mut state,
                &completion_engine,
                &services,
            ));
            timeout(Duration::from_secs(1), async {
                tokio::select! {
                    _ = &mut follow_up => {
                        panic!("follow-up ran before query task joined")
                    }
                    result = &mut query_drop_rx => {
                        result.expect("query drop signal should be sent");
                    }
                }
            })
            .await
            .expect("query task should enter its drop gate");
            query_drop_state.release();
            timeout(Duration::from_secs(1), &mut query_finished_rx)
                .await
                .expect("query task should leave its drop gate")
                .expect("query finished signal should be sent");
            timeout(Duration::from_secs(1), async {
                tokio::select! {
                    _ = &mut follow_up => {
                        panic!("follow-up ran before SQLite diagnostics task joined")
                    }
                    result = &mut diagnostics_drop_rx => {
                        result.expect("SQLite diagnostics drop signal should be sent");
                    }
                }
            })
            .await
            .expect("SQLite diagnostics task should enter its drop gate");
            assert!(
                timeout(Duration::from_millis(50), &mut follow_up)
                    .await
                    .is_err(),
                "follow-up ran before SQLite diagnostics task joined"
            );
            diagnostics_drop_state.release();
            let actions = timeout(Duration::from_secs(1), follow_up)
                .await
                .expect("cancellation and follow-up should finish")
                .expect("effect execution should succeed");
            let table_dropped = timeout(Duration::from_secs(1), &mut table_drop_rx)
                .await
                .is_ok();
            let connection_aborted = timeout(Duration::from_secs(1), &mut connection_drop_rx)
                .await
                .is_ok();
            assert!(connection_aborted, "connection task was not aborted");
            assert!(table_dropped, "table detail task was not aborted");
            assert!(matches!(actions.as_slice(), [Action::Render]));
            assert!(
                rx.try_recv().is_err(),
                "SQLite diagnostics task emitted a late action"
            );
        }
    }

    mod dispatch_actions {
        use super::*;

        #[tokio::test]
        async fn dispatches_all_actions() {
            let (tx, mut _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::DispatchActions(vec![
                    Action::ProcessPrefetchQueue { run_id: 1 },
                    Action::ProcessPrefetchQueue { run_id: 1 },
                ]),
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut _rx,
                None,
            )
            .await
            .unwrap();

            assert_eq!(run.actions.len(), 2);
            assert!(matches!(
                run.actions[0],
                Action::ProcessPrefetchQueue { run_id: 1 }
            ));
            assert!(matches!(
                run.actions[1],
                Action::ProcessPrefetchQueue { run_id: 1 }
            ));
        }
    }

    mod query_context_termination {
        use std::future::pending;
        use std::path::PathBuf;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};

        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::connection::{ConnectionId, DatabaseType};
        use crate::domain::{QueryResult, Table, WriteExecutionResult};
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

            async fn export_to_csv(
                &self,
                _dsn: &str,
                _query: &str,
                _file_name: &str,
            ) -> Result<PathBuf, DbOperationError> {
                unreachable!("test only starts a preview")
            }
        }

        struct PendingTableDetailProvider {
            started: Mutex<Option<oneshot::Sender<()>>>,
            dropped: Arc<AtomicBool>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingTableDetailProvider {
            async fn fetch_metadata(
                &self,
                _dsn: &str,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                unreachable!("test only starts table detail")
            }

            async fn fetch_table_detail(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                let _guard = DropSignal(Arc::clone(&self.dropped));
                self.started
                    .lock()
                    .expect("started signal lock poisoned")
                    .take()
                    .expect("table detail should start once")
                    .send(())
                    .ok();
                pending().await
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts table detail")
            }

            async fn fetch_table_signatures(
                &self,
                _dsn: &str,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                unreachable!("test only starts table detail")
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
                .insert(target_id.clone(), ConnectionCache::default());
            let run_id = state.query.begin_running(Instant::now());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
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
                    database: None,
                }),
                Instant::now(),
                &AppServices::stub(),
            );
            runner
                .execute_effects(
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

        #[tokio::test]
        async fn cancelling_context_drops_pending_table_detail_task() {
            let (started_tx, started_rx) = oneshot::channel();
            let dropped = Arc::new(AtomicBool::new(false));
            let provider = PendingTableDetailProvider {
                started: Mutex::new(Some(started_tx)),
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::FetchTableDetail {
                        dsn: "postgres://localhost/current".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        run_id: 1,
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
                .expect("pending table detail should start")
                .expect("started signal should be sent");

            let shutdown_effects = reduce(
                &mut state,
                Action::Quit,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.should_quit);
            runner
                .execute_effects(
                    shutdown_effects,
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
            .expect("context cancellation should drop the pending table detail task");
        }
    }

    mod metadata_context_termination {
        use std::future::pending;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::Table;
        use crate::domain::connection::{ConnectionId, DatabaseType};
        use crate::ports::outbound::DbOperationError;
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;

        struct DropSignal(Arc<AtomicUsize>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct PendingMetadataProvider {
            metadata_started: Mutex<Option<oneshot::Sender<()>>>,
            effective_user_started: Mutex<Option<oneshot::Sender<()>>>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingMetadataProvider {
            async fn fetch_metadata(
                &self,
                _dsn: &str,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _guard = DropSignal(Arc::clone(&self.dropped));
                self.metadata_started
                    .lock()
                    .expect("metadata started signal lock poisoned")
                    .take()
                    .expect("metadata should start once")
                    .send(())
                    .ok();
                pending().await
            }

            async fn fetch_effective_user(
                &self,
                _dsn: &str,
            ) -> Result<Option<String>, DbOperationError> {
                let _guard = DropSignal(Arc::clone(&self.dropped));
                self.effective_user_started
                    .lock()
                    .expect("effective user started signal lock poisoned")
                    .take()
                    .expect("effective user should start once")
                    .send(())
                    .ok();
                pending().await
            }

            async fn fetch_table_detail(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts metadata tasks")
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts metadata tasks")
            }

            async fn fetch_table_signatures(
                &self,
                _dsn: &str,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                unreachable!("test only starts metadata tasks")
            }
        }

        struct ProbeThatObservesDrop {
            metadata_dropped: Arc<AtomicUsize>,
            started: Mutex<Option<oneshot::Sender<bool>>>,
        }

        #[async_trait::async_trait]
        impl MySqlConnectionProbe for ProbeThatObservesDrop {
            async fn probe(
                &self,
                _dsn: &str,
            ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
                self.started
                    .lock()
                    .expect("probe started signal lock poisoned")
                    .take()
                    .expect("probe should start once")
                    .send(self.metadata_dropped.load(Ordering::SeqCst) > 0)
                    .ok();
                Ok(MySqlConnectionProbeResult {
                    lower_case_table_names: 0,
                })
            }
        }

        #[tokio::test]
        async fn drops_old_metadata_before_new_mysql_probe_starts() {
            let (metadata_started_tx, metadata_started_rx) = oneshot::channel();
            let (probe_started_tx, probe_started_rx) = oneshot::channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let metadata_provider = PendingMetadataProvider {
                metadata_started: Mutex::new(Some(metadata_started_tx)),
                effective_user_started: Mutex::new(None),
                dropped: Arc::clone(&dropped),
            };
            let probe = ProbeThatObservesDrop {
                metadata_dropped: Arc::clone(&dropped),
                started: Mutex::new(Some(probe_started_tx)),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(metadata_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(test_fixtures::NoopDsnBuilder),
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::FetchMetadata {
                        dsn: "postgres://localhost/old".to_string(),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            metadata_started_rx.await.expect("metadata should start");

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: ConnectionTarget {
                            id: ConnectionId::new(),
                            dsn: "mysql://localhost/new".to_string(),
                            name: "new".to_string(),
                            database_type: DatabaseType::MySQL,
                            database: Some("new".to_string()),
                        },
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(
                probe_started_rx
                    .await
                    .expect("probe should start after cancellation")
            );
            assert_eq!(dropped.load(Ordering::SeqCst), 1);
        }

        #[tokio::test]
        async fn quit_drops_effective_user_and_delayed_prefetch_tasks() {
            let (effective_user_started_tx, effective_user_started_rx) = oneshot::channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let provider = PendingMetadataProvider {
                metadata_started: Mutex::new(None),
                effective_user_started: Mutex::new(Some(effective_user_started_tx)),
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![
                        Effect::FetchEffectiveUser {
                            dsn: "postgres://localhost/current".to_string(),
                            run_id: 1,
                        },
                        Effect::DelayedProcessPrefetchQueue {
                            run_id: 1,
                            delay_secs: 60,
                        },
                    ],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            effective_user_started_rx
                .await
                .expect("effective user should start");

            let shutdown_effects = reduce(
                &mut state,
                Action::Quit,
                Instant::now(),
                &AppServices::stub(),
            );
            runner
                .execute_effects(
                    shutdown_effects,
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert!(
                timeout(Duration::from_millis(100), action_rx.recv())
                    .await
                    .is_err()
            );
        }
    }

    mod connection_task_lifecycle {
        use std::fs;
        use std::future::pending;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use tempfile::tempdir;
        use tokio::sync::mpsc::UnboundedSender;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::Table;
        use crate::domain::connection::{
            ConnectionConfig, ConnectionId, DatabaseType, MySqlConnectionConfig, MySqlSslMode,
            PostgresConnectionConfig, SqliteConnectionConfig, SslMode,
        };
        use crate::ports::outbound::DbOperationError;
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;

        struct DropSignal(Arc<AtomicUsize>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct PendingMySqlConnectionProbe {
            started: UnboundedSender<String>,
            dropped: Arc<AtomicUsize>,
        }

        struct PendingMetadataProvider {
            started: UnboundedSender<String>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MySqlConnectionProbe for PendingMySqlConnectionProbe {
            async fn probe(
                &self,
                dsn: &str,
            ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
                let _drop_signal = DropSignal(Arc::clone(&self.dropped));
                self.started
                    .send(dsn.to_string())
                    .expect("probe receiver should stay alive");
                pending().await
            }
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingMetadataProvider {
            async fn fetch_metadata(
                &self,
                dsn: &str,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _drop_signal = DropSignal(Arc::clone(&self.dropped));
                self.started
                    .send(dsn.to_string())
                    .expect("metadata receiver should stay alive");
                pending().await
            }

            async fn fetch_table_detail(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                pending().await
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                pending().await
            }

            async fn fetch_table_signatures(
                &self,
                _dsn: &str,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                pending().await
            }
        }

        fn mysql_target(dsn: &str) -> ConnectionTarget {
            ConnectionTarget {
                id: ConnectionId::new(),
                dsn: dsn.to_string(),
                name: dsn.to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("app".to_string()),
            }
        }

        fn mysql_config() -> ConnectionConfig {
            ConnectionConfig::MySQL(MySqlConnectionConfig::new(
                "localhost",
                3306,
                Some("app".to_string()),
                "user",
                "secret",
                MySqlSslMode::Required,
            ))
        }

        fn postgres_config() -> ConnectionConfig {
            ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "localhost",
                5432,
                "app",
                "user",
                "secret",
                SslMode::Prefer,
            ))
        }

        #[tokio::test]
        async fn replacing_probe_aborts_previous_task_before_starting_new_one() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let probe = PendingMySqlConnectionProbe {
                started: started_tx,
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(test_fixtures::NoopDsnBuilder),
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/old"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("mysql://localhost/old")
            );

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/new"),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("mysql://localhost/new")
            );

            runner
                .execute_effects(
                    vec![Effect::CancelTrackedTasks],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(dropped.load(Ordering::SeqCst), 2);
        }

        #[tokio::test]
        async fn replacing_save_probe_aborts_previous_task_before_new_probe() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let probe = PendingMySqlConnectionProbe {
                started: started_tx,
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(test_fixtures::NoopDsnBuilder),
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "old".to_string(),
                        config: mysql_config(),
                        run_id: 1,
                        run_guard: test_fixtures::active_connection_save_guard(1),
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(started_rx.recv().await.as_deref(), Some(""));

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/new"),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("mysql://localhost/new")
            );

            runner
                .execute_effects(
                    vec![Effect::CancelTrackedTasks],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(dropped.load(Ordering::SeqCst), 2);
        }

        #[tokio::test]
        async fn quitting_aborts_pending_mysql_probe_task() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let probe = PendingMySqlConnectionProbe {
                started: started_tx,
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(test_fixtures::NoopDsnBuilder),
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/pending"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            timeout(Duration::from_secs(1), started_rx.recv())
                .await
                .expect("pending probe should start")
                .expect("probe start signal should be sent");

            let shutdown_effects = reduce(
                &mut state,
                Action::Quit,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.should_quit);
            runner
                .execute_effects(
                    shutdown_effects,
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
        }

        #[tokio::test]
        async fn quitting_cancels_sqlite_save_before_blocking_claim() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();

            let (action_tx, mut action_rx) = mpsc::channel(8);
            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                action_tx,
                Arc::new(test_fixtures::NoopDsnBuilder),
            );
            let mut state = AppState::new("test".to_string());
            let run_id = state.session.begin_connection_save();
            let run_guard = state.session.connection_save_guard();
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "Local".to_string(),
                        config: ConnectionConfig::SQLite(
                            SqliteConnectionConfig::new(path.to_string_lossy().to_string())
                                .unwrap(),
                        ),
                        run_id,
                        run_guard,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let shutdown_effects = reduce(
                &mut state,
                Action::Quit,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.should_quit);
            tokio::task::yield_now().await;
            runner
                .execute_effects(
                    shutdown_effects,
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(
                timeout(Duration::from_millis(100), action_rx.recv())
                    .await
                    .is_err()
            );
        }

        #[tokio::test]
        async fn cancelling_postgres_save_aborts_metadata_before_action() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let provider = PendingMetadataProvider {
                started: started_tx,
                dropped: Arc::clone(&dropped),
            };
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            );
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "PostgreSQL".to_string(),
                        config: postgres_config(),
                        run_id: 1,
                        run_guard: test_fixtures::active_connection_save_guard(1),
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(started_rx.recv().await.as_deref(), Some(""));

            runner
                .execute_effects(
                    vec![Effect::CancelConnectionTask],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert!(
                timeout(Duration::from_millis(100), action_rx.recv())
                    .await
                    .is_err()
            );
        }
    }

    mod smart_er_refresh_lifecycle {
        use std::future::pending;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use tokio::sync::mpsc::UnboundedSender;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::Table;
        use crate::ports::outbound::DbOperationError;
        use crate::update::reducer::reduce;

        struct DropSignal(Arc<AtomicUsize>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct PendingSmartErProvider {
            started: UnboundedSender<String>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingSmartErProvider {
            async fn fetch_metadata(
                &self,
                dsn: &str,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _guard = DropSignal(Arc::clone(&self.dropped));
                self.started.send(dsn.to_string()).ok();
                pending().await
            }

            async fn fetch_table_detail(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts smart ER refresh")
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _dsn: &str,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts smart ER refresh")
            }

            async fn fetch_table_signatures(
                &self,
                _dsn: &str,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                unreachable!("test only starts smart ER refresh")
            }
        }

        async fn wait_for_no_action(action_rx: &mut mpsc::Receiver<Action>) {
            assert!(
                timeout(Duration::from_millis(100), action_rx.recv())
                    .await
                    .is_err()
            );
        }

        fn runner_with_pending_provider(
            started: UnboundedSender<String>,
            dropped: Arc<AtomicUsize>,
            action_tx: mpsc::Sender<Action>,
        ) -> EffectRunner {
            test_fixtures::make_runner(
                Arc::new(PendingSmartErProvider { started, dropped }),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            )
        }

        #[tokio::test]
        async fn rerun_aborts_previous_refresh_before_starting_new_one() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = runner_with_pending_provider(started_tx, Arc::clone(&dropped), action_tx);
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        dsn: "postgres://localhost/old".to_string(),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("postgres://localhost/old")
            );

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        dsn: "postgres://localhost/new".to_string(),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("postgres://localhost/new")
            );
            wait_for_no_action(&mut action_rx).await;
        }

        #[tokio::test]
        async fn connection_switch_aborts_pending_refresh_and_emits_no_action() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = runner_with_pending_provider(started_tx, Arc::clone(&dropped), action_tx);
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        dsn: "postgres://localhost/current".to_string(),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("postgres://localhost/current")
            );

            runner
                .execute_effects(
                    vec![Effect::SwitchConnection {
                        connection_index: usize::MAX,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            wait_for_no_action(&mut action_rx).await;
        }

        #[tokio::test]
        async fn metadata_refresh_aborts_pending_smart_er_refresh() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = runner_with_pending_provider(started_tx, Arc::clone(&dropped), action_tx);
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        dsn: "postgres://localhost/current".to_string(),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("postgres://localhost/current")
            );

            runner
                .execute_effects(
                    vec![Effect::FetchMetadata {
                        dsn: "postgres://localhost/current".to_string(),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            wait_for_no_action(&mut action_rx).await;
        }

        #[tokio::test]
        async fn quit_aborts_pending_refresh_and_emits_no_action() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = runner_with_pending_provider(started_tx, Arc::clone(&dropped), action_tx);
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        dsn: "postgres://localhost/current".to_string(),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await.as_deref(),
                Some("postgres://localhost/current")
            );

            let shutdown_effects = reduce(
                &mut state,
                Action::Quit,
                Instant::now(),
                &AppServices::stub(),
            );
            assert!(state.should_quit);
            runner
                .execute_effects(
                    shutdown_effects,
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            wait_for_no_action(&mut action_rx).await;
        }
    }
}
