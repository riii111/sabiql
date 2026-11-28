use std::sync::Arc;
use std::time::Instant;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::browse::metadata as cmd_metadata;
use crate::cmd::browse::query::{self as cmd_query, QueryHistoryTarget};
use crate::cmd::connection::{self as cmd_connection, SaveAndConnectRequest};
use crate::cmd::effect::Effect;
use crate::cmd::er::handler as cmd_er;
use crate::cmd::er::task::{self as cmd_er_task, ErDiagramRequest};
use crate::cmd::metadata_task::MetadataTaskRegistry;
use crate::cmd::settings as cmd_settings;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::cmd::sql_editor::query_history::load_query_history;
use crate::cmd::sqlite_diagnostics;
use crate::cmd::utility as cmd_utility;
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    CachedResultExporter, ClipboardWriter, ConfigWriter, ConnectionStore, ErDiagramExporter,
    ErLogWriter, FolderOpener, MetadataProvider, MySqlConnectionProbe, PgServiceEntryReader,
    QueryExecutor, QueryHistoryStore, Renderer, SettingsStore, SqliteDiagnosticsProvider,
    SqlitePathValidator,
};
use crate::services::AppServices;
use crate::update::action::{Action, QueryContext};

#[derive(Clone)]
pub struct ConnectionDeps {
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
    connection_task: SingleTaskOwner,
    sqlite_diagnostics_task: sqlite_diagnostics::SqliteDiagnosticsTaskOwner,
    smart_er_refresh_task: SingleTaskOwner,
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
            connection_task: SingleTaskOwner::default(),
            sqlite_diagnostics_task: sqlite_diagnostics::SqliteDiagnosticsTaskOwner::default(),
            smart_er_refresh_task: SingleTaskOwner::default(),
        }
    }

    pub async fn execute_effects<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        tui: &mut T,
        state: &mut AppState,
        services: &AppServices,
    ) -> Result<()> {
        for effect in effects {
            self.execute_single_effect(effect, tui, state, services)
                .await?;
        }
        Ok(())
    }

    fn report<A>(
        &self,
        task: impl Future<Output = A> + Send + 'static,
    ) -> impl Future<Output = ()> + Send + 'static
    where
        A: Into<Option<Action>> + Send + 'static,
    {
        let tx = self.action_tx.clone();
        async move {
            if let Some(action) = task.await.into() {
                tx.send(action).await.ok();
            }
        }
    }

    fn spawn_report<A>(&self, task: impl Future<Output = A> + Send + 'static)
    where
        A: Into<Option<Action>> + Send + 'static,
    {
        tokio::spawn(self.report(task));
    }

    fn spawn_metadata_report(&self, task: impl Future<Output = Action> + Send + 'static) {
        MetadataTaskRegistry::spawn(&self.metadata_tasks, self.report(task));
    }

    async fn execute_single_effect<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        services: &AppServices,
    ) -> Result<()> {
        let metadata = || Arc::clone(&self.metadata_provider);
        let executor = || Arc::clone(&self.query.query_executor);
        match effect {
            Effect::Render => {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "the effect runner is the runtime boundary that reads the clock for rendering"
                )]
                let now = Instant::now();
                let output = tui.draw(state, services, now)?;
                state.apply_render_output(output);
                state.motion.note_frame(now);
            }

            Effect::CopyToClipboard { content, target } => {
                self.spawn_report(cmd_utility::copy_to_clipboard(
                    Arc::clone(&self.utility.clipboard),
                    content,
                    target,
                ));
            }
            Effect::OpenFolder { path } => {
                let failure = cmd_utility::open_folder(self.utility.folder_opener.as_ref(), &path);
                self.spawn_report(async move { failure });
            }

            Effect::SaveAndConnect {
                id,
                name,
                config,
                run_id,
                run_guard,
            } => {
                let task = cmd_connection::save_and_connect(
                    self.connection.clone(),
                    metadata(),
                    SaveAndConnectRequest {
                        id,
                        name,
                        config,
                        run_id,
                        run_guard,
                    },
                );
                self.connection_task.replace(self.report(task)).await;
            }
            Effect::ProbeMySqlConnection { target, run_id } => {
                let task = cmd_connection::probe_mysql_connection(
                    Arc::clone(&self.connection.mysql_connection_probe),
                    target,
                    run_id,
                );
                self.connection_task.replace(self.report(task)).await;
            }
            Effect::LoadConnectionForEdit { id } => {
                self.spawn_report(cmd_connection::load_connection_for_edit(
                    Arc::clone(&self.connection.connection_store),
                    id,
                ));
            }
            Effect::LoadConnections => {
                self.spawn_report(cmd_connection::load_connections(
                    Arc::clone(&self.connection.connection_store),
                    Arc::clone(&self.connection.pg_service_entry_reader),
                ));
            }
            Effect::DeleteConnection { id } => {
                self.spawn_report(cmd_connection::delete_connection(
                    Arc::clone(&self.connection.connection_store),
                    id,
                ));
            }

            Effect::FetchMetadata { endpoint, run_id } => {
                self.spawn_metadata_report(cmd_metadata::fetch_metadata(
                    metadata(),
                    endpoint,
                    run_id,
                ));
            }
            Effect::FetchEffectiveUser { endpoint, run_id } => {
                self.spawn_metadata_report(cmd_metadata::fetch_effective_user(
                    metadata(),
                    endpoint,
                    run_id,
                ));
            }
            Effect::FetchTableDetail {
                endpoint,
                schema,
                table,
                generation,
                run_id,
            } => {
                let task = cmd_metadata::fetch_table_detail(
                    metadata(),
                    endpoint,
                    schema,
                    table,
                    generation,
                    run_id,
                );
                self.table_detail_tasks.replace(self.report(task)).await;
            }
            Effect::PrefetchTableColumnsAndFks {
                endpoint,
                run_id,
                schema,
                table,
            } => {
                self.spawn_metadata_report(cmd_metadata::prefetch_table_detail(
                    metadata(),
                    endpoint,
                    run_id,
                    schema,
                    table,
                ));
            }
            Effect::SchedulePrefetchQueueProcessing { run_id } => {
                self.spawn_report(async move { Action::ProcessPrefetchQueue { run_id } });
            }
            Effect::DelayedProcessPrefetchQueue { run_id, delay_secs } => {
                self.spawn_metadata_report(cmd_metadata::process_prefetch_queue_after(
                    delay_secs, run_id,
                ));
            }

            Effect::CancelConnectionTask => self.connection_task.cancel().await,
            Effect::CancelMetadataTasks => {
                self.metadata_tasks.cancel().await;
                self.smart_er_refresh_task.cancel().await;
            }
            Effect::CancelTableDetail => self.table_detail_tasks.cancel().await,
            Effect::CancelSqliteDiagnostics => self.sqlite_diagnostics_task.cancel().await,
            Effect::CancelTrackedTasks => self.cancel_tracked_tasks().await,

            Effect::ExecutePreview {
                endpoint,
                schema,
                table,
                generation,
                run_id,
                limit,
                offset,
                target_page,
            } => {
                let task = cmd_query::execute_preview(
                    executor(),
                    endpoint,
                    schema,
                    table,
                    run_id,
                    QueryContext::Preview {
                        generation,
                        target_page,
                    },
                    limit,
                    offset,
                );
                self.query_tasks.replace(self.report(task)).await;
            }
            Effect::LoadTable {
                endpoint,
                schema,
                table,
                generation,
                detail_run_id,
                preview_run_id,
                limit,
                offset,
                target_page,
            } => {
                let detail = cmd_metadata::fetch_table_detail(
                    metadata(),
                    Arc::clone(&endpoint),
                    schema.clone(),
                    table.clone(),
                    generation,
                    detail_run_id,
                );
                let preview = cmd_query::execute_preview(
                    executor(),
                    endpoint,
                    schema,
                    table,
                    preview_run_id,
                    QueryContext::Preview {
                        generation,
                        target_page,
                    },
                    limit,
                    offset,
                );
                // The preview supersedes the running query, which is stopped here because the detail task does not own the query slot.
                self.query_tasks.cancel().await;
                let tx = self.action_tx.clone();
                // The detail task owns the preview so a later query cannot abort the detail, and the detail is sent first so the preview never shows before it settles.
                self.table_detail_tasks
                    .replace(async move {
                        let send_detail = async {
                            tx.send(detail.await).await.ok();
                        };
                        let ((), preview) = tokio::join!(send_detail, preview);
                        tx.send(preview).await.ok();
                    })
                    .await;
            }
            Effect::ExecuteAdhoc {
                endpoint,
                run_id,
                query,
                access_mode,
            } => {
                let task = cmd_query::execute_adhoc(
                    executor(),
                    QueryHistoryTarget::for_state(&self.query.query_history_store, state),
                    endpoint,
                    run_id,
                    query,
                    access_mode,
                );
                self.query_tasks.replace(self.report(task)).await;
            }
            Effect::ExecuteExplain {
                endpoint,
                database_generation,
                run_id,
                query,
                source_query,
                is_analyze,
                access_mode,
            } => {
                let task = cmd_query::execute_explain(
                    executor(),
                    endpoint,
                    database_generation,
                    run_id,
                    query,
                    source_query,
                    is_analyze,
                    access_mode,
                );
                self.query_tasks.replace(self.report(task)).await;
            }
            Effect::ExecuteWrite {
                endpoint,
                run_id,
                query,
                access_mode,
            } => {
                let task = cmd_query::execute_write(
                    executor(),
                    QueryHistoryTarget::for_state(&self.query.query_history_store, state),
                    endpoint,
                    run_id,
                    query,
                    access_mode,
                );
                self.query_tasks.replace(self.report(task)).await;
            }
            Effect::ExportCsvFromCache {
                run_id,
                file_name,
                result,
            } => {
                let task = cmd_query::export_csv(
                    Arc::clone(&self.query.cached_result_exporter),
                    run_id,
                    file_name,
                    result,
                );
                self.query_tasks.replace(self.report(task)).await;
            }

            Effect::GenerateErDiagram {
                run_id,
                total_tables,
                project_name,
                tables,
                filename,
            } => {
                let request = ErDiagramRequest {
                    run_id,
                    total_tables,
                    tables,
                    cache_dir: self.er.config_writer.get_cache_dir(&project_name)?,
                    filename,
                    browser: state.settings.saved_er_browser().map(str::to_string),
                };
                self.spawn_report(cmd_er_task::generate_er_diagram(
                    Arc::clone(&self.er.er_exporter),
                    request,
                ));
            }
            Effect::WriteErFailureLog { failed_tables } => {
                self.spawn_report(cmd_er::write_failure_log(
                    Arc::clone(&self.er.er_log_writer),
                    self.er.config_writer.get_cache_dir(state.project_name()),
                    failed_tables,
                ));
            }
            Effect::SmartErRefresh { endpoint, run_id } => {
                let task = cmd_er::smart_refresh(metadata(), endpoint, run_id);
                self.smart_er_refresh_task.replace(self.report(task)).await;
            }

            Effect::LoadQueryHistory {
                project_name,
                scope,
            } => {
                self.spawn_report(load_query_history(
                    Arc::clone(&self.query.query_history_store),
                    project_name,
                    scope,
                ));
            }

            Effect::SaveSettings { settings } => {
                // Saving and reporting in effect order keeps a slower earlier save from overwriting a later one.
                let action = cmd_settings::save(settings, self.settings_store.as_ref());
                self.action_tx.send(action).await.ok();
            }

            Effect::FetchSqliteDiagnosticsCore { endpoint, run_id } => {
                let task = sqlite_diagnostics::fetch_core(
                    Arc::clone(&self.query.sqlite_diagnostics),
                    endpoint,
                    run_id,
                );
                self.sqlite_diagnostics_task
                    .core
                    .replace(self.report(task))
                    .await;
            }
            Effect::FetchSqliteDiagnosticsQuickCheck { endpoint, run_id } => {
                let task = sqlite_diagnostics::fetch_quick_check(
                    Arc::clone(&self.query.sqlite_diagnostics),
                    endpoint,
                    run_id,
                );
                self.sqlite_diagnostics_task
                    .quick_check
                    .replace(self.report(task))
                    .await;
            }
        }
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::test_fixtures::{self, DropCounter, DropFlag, NoopRenderer};
    use crate::domain::connection::ConnectionEndpoint;
    use crate::domain::{DatabaseMetadata, TableSignatureSnapshot, TableSummary};
    use crate::model::shared::render_output::{
        BrowseLayout, DetailLayout, ExplorerLayout, JsonDetailLayout,
    };
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{MySqlConnectionProbeResult, RenderOutput, RenderResult};
    use crate::services::AppServices;
    use crate::test_support::endpoint;
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
                )];
                metadata
            })));
            state.ui.explorer_horizontal_offset = 20;

            let mut renderer = ExplorerWidthRenderer {
                explorer_content_width: 8,
            };

            runner
                .execute_effects(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(state.ui.explorer_horizontal_offset, 9);
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

            let mut renderer = JsonVisibleRowsRenderer { visible_rows: 2 };

            runner
                .execute_effects(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(state.ui.json_detail_editor_visible_rows, 2);
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
                _endpoint: &ConnectionEndpoint,
            ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
                let _drop_signal = BlockingDrop(Arc::clone(&self.drop_state));
                self.started.notify_one();
                pending().await
            }

            async fn fetch_quick_check(&self, _endpoint: &ConnectionEndpoint) -> DiagnosticField {
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
            runner
                .sqlite_diagnostics_task
                .core
                .replace(runner.report(sqlite_diagnostics::fetch_core(
                    diagnostics_provider,
                    endpoint::sqlite("sqlite:///tmp/app.db"),
                    1,
                )))
                .await;
            timeout(Duration::from_secs(1), diagnostics_started.notified())
                .await
                .expect("SQLite diagnostics task should start");

            let mut state = AppState::new("test".to_string());
            let mut renderer = test_fixtures::NoopRenderer;
            let services = AppServices::stub();
            let mut cancellation = Box::pin(runner.execute_effects(
                vec![Effect::CancelTrackedTasks],
                &mut renderer,
                &mut state,
                &services,
            ));
            timeout(Duration::from_secs(1), async {
                tokio::select! {
                    _ = &mut cancellation => {
                        panic!("cancellation finished before query task joined")
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
                    _ = &mut cancellation => {
                        panic!("cancellation finished before SQLite diagnostics task joined")
                    }
                    result = &mut diagnostics_drop_rx => {
                        result.expect("SQLite diagnostics drop signal should be sent");
                    }
                }
            })
            .await
            .expect("SQLite diagnostics task should enter its drop gate");
            assert!(
                timeout(Duration::from_millis(50), &mut cancellation)
                    .await
                    .is_err(),
                "cancellation finished before SQLite diagnostics task joined"
            );
            diagnostics_drop_state.release();
            timeout(Duration::from_secs(1), cancellation)
                .await
                .expect("cancellation should finish")
                .expect("effect execution should succeed");
            let table_dropped = timeout(Duration::from_secs(1), &mut table_drop_rx)
                .await
                .is_ok();
            let connection_aborted = timeout(Duration::from_secs(1), &mut connection_drop_rx)
                .await
                .is_ok();
            assert!(connection_aborted, "connection task was not aborted");
            assert!(table_dropped, "table detail task was not aborted");
            assert!(
                rx.try_recv().is_err(),
                "SQLite diagnostics task emitted a late action"
            );
        }
    }

    mod query_context_termination {
        use std::future::pending;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};

        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::connection::ConnectionId;
        use crate::domain::{QueryResult, Table, WriteExecutionResult};
        use crate::model::connection::cache::ConnectionCache;
        use crate::ports::outbound::{AccessMode, DbOperationError};
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;
        use crate::update::test_fixtures::switch_connection;

        struct PendingQueryExecutor {
            started: Mutex<Option<oneshot::Sender<()>>>,
            dropped: Arc<AtomicBool>,
        }

        #[async_trait::async_trait]
        impl QueryExecutor for PendingQueryExecutor {
            async fn execute_preview(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
                _limit: usize,
                _offset: usize,
            ) -> Result<QueryResult, DbOperationError> {
                let _guard = DropFlag(Arc::clone(&self.dropped));
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
                _endpoint: &ConnectionEndpoint,
                _query: &str,
                _access_mode: AccessMode,
            ) -> Result<QueryResult, DbOperationError> {
                unreachable!("test only starts a preview")
            }

            async fn execute_write(
                &self,
                _endpoint: &ConnectionEndpoint,
                _query: &str,
                _access_mode: AccessMode,
            ) -> Result<WriteExecutionResult, DbOperationError> {
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
                _endpoint: &ConnectionEndpoint,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                unreachable!("test only starts table detail")
            }

            async fn fetch_table_detail(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                let _guard = DropFlag(Arc::clone(&self.dropped));
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
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts table detail")
            }

            async fn fetch_table_signatures(
                &self,
                _endpoint: &ConnectionEndpoint,
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
            state.session.activate_connection(
                &current_id,
                "current",
                &endpoint::postgres("postgres://localhost/current"),
            );
            let target_id = ConnectionId::new();
            state
                .connection_caches
                .insert(target_id.clone(), ConnectionCache::default());
            let run_id = state.query.begin_running(Instant::now());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::ExecutePreview {
                        endpoint: endpoint::postgres("postgres://localhost/current"),
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
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            timeout(Duration::from_secs(1), started_rx)
                .await
                .expect("pending query should start")
                .expect("started signal should be sent");

            let effects = switch_connection(
                &mut state,
                ConnectionTarget {
                    id: target_id,
                    name: "target".to_string(),
                    endpoint: endpoint::sqlite("sqlite:///tmp/target.db"),
                },
            );
            runner
                .execute_effects(effects, &mut renderer, &mut state, &AppServices::stub())
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
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::FetchTableDetail {
                        endpoint: endpoint::postgres("postgres://localhost/current"),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
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

    mod settings_save {
        use super::*;
        use crate::model::shared::theme_id::ThemeId;
        use crate::ports::outbound::AppSettings;

        #[tokio::test]
        async fn reports_saves_in_effect_order() {
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                tx,
            );
            let settings = |theme_id| AppSettings {
                theme_id,
                ..AppSettings::default()
            };

            runner
                .execute_effects(
                    vec![
                        Effect::SaveSettings {
                            settings: settings(ThemeId::Light),
                        },
                        Effect::SaveSettings {
                            settings: settings(ThemeId::Default),
                        },
                    ],
                    &mut NoopRenderer,
                    &mut AppState::new("test".to_string()),
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let themes =
                [rx.try_recv().unwrap(), rx.try_recv().unwrap()].map(|action| match action {
                    Action::SettingsSaved(Ok(saved)) => saved.theme_id,
                    other => panic!("expected saved settings, got {other:?}"),
                });
            assert_eq!(themes, [ThemeId::Light, ThemeId::Default]);
        }
    }

    mod table_load {
        use tokio::sync::Notify;
        use tokio::time::{Duration, timeout};

        use super::*;
        use crate::domain::Table;
        use crate::ports::outbound::DbOperationError;
        use crate::test_support;

        struct GatedTableDetailProvider {
            release: Arc<Notify>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for GatedTableDetailProvider {
            async fn fetch_metadata(
                &self,
                _endpoint: &ConnectionEndpoint,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                unreachable!("test only loads a table")
            }

            async fn fetch_table_detail(
                &self,
                _endpoint: &ConnectionEndpoint,
                schema: &str,
                table: &str,
            ) -> Result<Table, DbOperationError> {
                self.release.notified().await;
                Ok(test_support::table::minimal(schema, table))
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only loads a table")
            }

            async fn fetch_table_signatures(
                &self,
                _endpoint: &ConnectionEndpoint,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                unreachable!("test only loads a table")
            }
        }

        #[tokio::test]
        async fn replaces_the_running_query() {
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_table_detail()
                .returning(|_, schema, table| Ok(test_support::table::minimal(schema, table)));
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_preview()
                .returning(|_, _, _, _, _| Ok(test_fixtures::sample_query_result()));
            let (tx, _rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(executor),
                Arc::new(MockConnectionStore::new()),
                tx,
            );
            let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            runner
                .query_tasks
                .replace({
                    let dropped = Arc::clone(&dropped);
                    async move {
                        let _guard = DropFlag(dropped);
                        started_tx.send(()).ok();
                        std::future::pending::<()>().await;
                    }
                })
                .await;
            started_rx.await.unwrap();

            runner
                .execute_effects(
                    vec![Effect::LoadTable {
                        endpoint: endpoint::postgres("postgres://localhost/test"),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        detail_run_id: 2,
                        preview_run_id: 3,
                        limit: 500,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut NoopRenderer,
                    &mut AppState::new("test".to_string()),
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
        }

        #[tokio::test]
        async fn delivers_detail_before_an_earlier_finished_preview() {
            let release = Arc::new(Notify::new());
            let mut executor = MockQueryExecutor::new();
            executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| Ok(test_fixtures::sample_query_result()));
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(GatedTableDetailProvider {
                    release: Arc::clone(&release),
                }),
                Arc::new(executor),
                Arc::new(MockConnectionStore::new()),
                tx,
            );

            runner
                .execute_effects(
                    vec![Effect::LoadTable {
                        endpoint: endpoint::postgres("postgres://localhost/test"),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        detail_run_id: 2,
                        preview_run_id: 3,
                        limit: 500,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut NoopRenderer,
                    &mut AppState::new("test".to_string()),
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert!(
                timeout(Duration::from_millis(50), rx.recv()).await.is_err(),
                "preview was delivered before the table detail"
            );
            release.notify_one();

            let first =
                test_fixtures::recv_action_with_timeout(&mut rx, Duration::from_secs(1)).await;
            let second =
                test_fixtures::recv_action_with_timeout(&mut rx, Duration::from_secs(1)).await;
            assert!(matches!(
                first,
                Action::TableDetailLoaded {
                    run_id: 2,
                    generation: 1,
                    outcome: Ok(_),
                    ..
                }
            ));
            assert!(matches!(
                second,
                Action::QueryCompleted {
                    run_id: 3,
                    context: QueryContext::Preview {
                        generation: 1,
                        target_page: 0
                    },
                    outcome: Ok(_),
                }
            ));
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
        use crate::domain::connection::ConnectionId;
        use crate::ports::outbound::DbOperationError;
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;

        struct PendingMetadataProvider {
            metadata_started: Mutex<Option<oneshot::Sender<()>>>,
            effective_user_started: Mutex<Option<oneshot::Sender<()>>>,
            effective_user_result: Mutex<Option<oneshot::Receiver<String>>>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingMetadataProvider {
            async fn fetch_metadata(
                &self,
                _endpoint: &ConnectionEndpoint,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _guard = DropCounter(Arc::clone(&self.dropped));
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
                _endpoint: &ConnectionEndpoint,
            ) -> Result<Option<String>, DbOperationError> {
                let _guard = DropCounter(Arc::clone(&self.dropped));
                self.effective_user_started
                    .lock()
                    .expect("effective user started signal lock poisoned")
                    .take()
                    .expect("effective user should start once")
                    .send(())
                    .ok();
                let result = self.effective_user_result.lock().unwrap().take();
                match result {
                    Some(result) => Ok(Some(result.await.unwrap())),
                    None => pending().await,
                }
            }

            async fn fetch_table_detail(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts metadata tasks")
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts metadata tasks")
            }

            async fn fetch_table_signatures(
                &self,
                _endpoint: &ConnectionEndpoint,
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
                _endpoint: &ConnectionEndpoint,
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
                effective_user_result: Mutex::new(None),
                dropped: Arc::clone(&dropped),
            };
            let probe = ProbeThatObservesDrop {
                metadata_dropped: Arc::clone(&dropped),
                started: Mutex::new(Some(probe_started_tx)),
            };
            let (action_tx, _action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(metadata_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::FetchMetadata {
                        endpoint: endpoint::postgres("postgres://localhost/old"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            metadata_started_rx.await.expect("metadata should start");

            runner
                .execute_effects(
                    vec![
                        Effect::CancelMetadataTasks,
                        Effect::ProbeMySqlConnection {
                            target: ConnectionTarget {
                                id: ConnectionId::new(),
                                name: "new".to_string(),
                                endpoint: endpoint::mysql("mysql://localhost/new", Some("new")),
                            },
                            run_id: 2,
                        },
                    ],
                    &mut renderer,
                    &mut state,
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
        async fn removed_table_allows_new_effective_user_result_after_old_tasks_join() {
            let (started_tx, started_rx) = oneshot::channel();
            let (result_tx, result_rx) = oneshot::channel();
            let provider = PendingMetadataProvider {
                metadata_started: Mutex::new(None),
                effective_user_started: Mutex::new(Some(started_tx)),
                effective_user_result: Mutex::new(Some(result_rx)),
                dropped: Arc::new(AtomicUsize::new(0)),
            };
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            );
            let old_dropped = Arc::new(AtomicUsize::new(0));
            let (old_started_tx, old_started_rx) = oneshot::channel();
            runner
                .query_tasks
                .replace({
                    let dropped = Arc::clone(&old_dropped);
                    async move {
                        let _guard = DropCounter(dropped);
                        old_started_tx.send(()).unwrap();
                        pending::<()>().await;
                    }
                })
                .await;
            old_started_rx.await.unwrap();
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "test",
                &endpoint::postgres("postgres://localhost/test"),
            );
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            let run_id = state.session.begin_metadata_refresh();
            let services = AppServices::stub();
            let effects = reduce(
                &mut state,
                Action::MetadataLoaded {
                    run_id,
                    outcome: Ok(Arc::new(DatabaseMetadata::new("test".to_string()))),
                },
                Instant::now(),
                &services,
            );

            runner
                .execute_effects(effects, &mut NoopRenderer, &mut state, &services)
                .await
                .unwrap();
            timeout(Duration::from_secs(1), started_rx)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(old_dropped.load(Ordering::SeqCst), 1);
            result_tx.send("refreshed_user".to_string()).unwrap();
            let action = timeout(Duration::from_secs(1), action_rx.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(action, Action::EffectiveUserLoaded { .. }));
            reduce(&mut state, action, Instant::now(), &services);

            assert_eq!(state.session.effective_user(), Some("refreshed_user"));
            assert!(state.session.selected_table_key().is_none());
        }

        #[tokio::test]
        async fn quit_drops_effective_user_and_delayed_prefetch_tasks() {
            let (effective_user_started_tx, effective_user_started_rx) = oneshot::channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let provider = PendingMetadataProvider {
                metadata_started: Mutex::new(None),
                effective_user_started: Mutex::new(Some(effective_user_started_tx)),
                effective_user_result: Mutex::new(None),
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
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![
                        Effect::FetchEffectiveUser {
                            endpoint: endpoint::postgres("postgres://localhost/current"),
                            run_id: 1,
                        },
                        Effect::DelayedProcessPrefetchQueue {
                            run_id: 1,
                            delay_secs: 60,
                        },
                    ],
                    &mut renderer,
                    &mut state,
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
            ConnectionConfig, ConnectionId, MySqlConnectionConfig, MySqlSslMode,
            PostgresConnectionConfig, SqliteConnectionConfig, SslMode,
        };
        use crate::ports::outbound::DbOperationError;
        use crate::update::action::ConnectionTarget;
        use crate::update::reducer::reduce;

        struct PendingMySqlConnectionProbe {
            started: UnboundedSender<Arc<ConnectionEndpoint>>,
            dropped: Arc<AtomicUsize>,
        }

        struct PendingMetadataProvider {
            started: UnboundedSender<Arc<ConnectionEndpoint>>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MySqlConnectionProbe for PendingMySqlConnectionProbe {
            async fn probe(
                &self,
                endpoint: &ConnectionEndpoint,
            ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
                let _drop_signal = DropCounter(Arc::clone(&self.dropped));
                self.started
                    .send(Arc::new(endpoint.clone()))
                    .expect("probe receiver should stay alive");
                pending().await
            }
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingMetadataProvider {
            async fn fetch_metadata(
                &self,
                endpoint: &ConnectionEndpoint,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _drop_signal = DropCounter(Arc::clone(&self.dropped));
                self.started
                    .send(Arc::new(endpoint.clone()))
                    .expect("metadata receiver should stay alive");
                pending().await
            }

            async fn fetch_table_detail(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                pending().await
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                pending().await
            }

            async fn fetch_table_signatures(
                &self,
                _endpoint: &ConnectionEndpoint,
            ) -> Result<TableSignatureSnapshot, DbOperationError> {
                pending().await
            }
        }

        fn mysql_target(dsn: &str) -> ConnectionTarget {
            ConnectionTarget {
                id: ConnectionId::new(),
                name: dsn.to_string(),
                endpoint: endpoint::mysql(dsn, Some("app")),
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
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/old"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::mysql("mysql://localhost/old", Some("app")))
            );

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/new"),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::mysql("mysql://localhost/new", Some("app")))
            );

            runner
                .execute_effects(
                    vec![Effect::CancelTrackedTasks],
                    &mut renderer,
                    &mut state,
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
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
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
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(Arc::new(ConnectionEndpoint::from(&mysql_config())))
            );

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/new"),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::mysql("mysql://localhost/new", Some("app")))
            );

            runner
                .execute_effects(
                    vec![Effect::CancelTrackedTasks],
                    &mut renderer,
                    &mut state,
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
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(probe),
            );
            let mut state = AppState::new("test".to_string());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::ProbeMySqlConnection {
                        target: mysql_target("mysql://localhost/pending"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
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
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
        }

        #[tokio::test]
        async fn quitting_cancels_sqlite_save_before_persistence() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();

            let (action_tx, mut action_rx) = mpsc::channel(8);
            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                action_tx,
            );
            let mut state = AppState::new("test".to_string());
            let run_id = state.session.begin_connection_save();
            let run_guard = state.session.connection_save_guard();
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
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(Arc::new(ConnectionEndpoint::from(&postgres_config())))
            );

            runner
                .execute_effects(
                    vec![Effect::CancelConnectionTask],
                    &mut renderer,
                    &mut state,
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

        struct PendingSmartErProvider {
            started: UnboundedSender<Arc<ConnectionEndpoint>>,
            dropped: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl MetadataProvider for PendingSmartErProvider {
            async fn fetch_metadata(
                &self,
                endpoint: &ConnectionEndpoint,
            ) -> Result<DatabaseMetadata, DbOperationError> {
                let _guard = DropCounter(Arc::clone(&self.dropped));
                self.started.send(Arc::new(endpoint.clone())).ok();
                pending().await
            }

            async fn fetch_table_detail(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts smart ER refresh")
            }

            async fn fetch_table_columns_and_fks(
                &self,
                _endpoint: &ConnectionEndpoint,
                _schema: &str,
                _table: &str,
            ) -> Result<Table, DbOperationError> {
                unreachable!("test only starts smart ER refresh")
            }

            async fn fetch_table_signatures(
                &self,
                _endpoint: &ConnectionEndpoint,
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
            started: UnboundedSender<Arc<ConnectionEndpoint>>,
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
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        endpoint: endpoint::postgres("postgres://localhost/old"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::postgres("postgres://localhost/old"))
            );

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        endpoint: endpoint::postgres("postgres://localhost/new"),
                        run_id: 2,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::postgres("postgres://localhost/new"))
            );
            wait_for_no_action(&mut action_rx).await;
        }

        #[tokio::test]
        async fn metadata_cancellation_aborts_pending_smart_er_refresh() {
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicUsize::new(0));
            let (action_tx, mut action_rx) = mpsc::channel(8);
            let runner = runner_with_pending_provider(started_tx, Arc::clone(&dropped), action_tx);
            let mut state = AppState::new("test".to_string());
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        endpoint: endpoint::postgres("postgres://localhost/current"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::postgres("postgres://localhost/current"))
            );

            runner
                .execute_effects(
                    vec![Effect::CancelMetadataTasks],
                    &mut renderer,
                    &mut state,
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
            let mut renderer = NoopRenderer;

            runner
                .execute_effects(
                    vec![Effect::SmartErRefresh {
                        endpoint: endpoint::postgres("postgres://localhost/current"),
                        run_id: 1,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
            assert_eq!(
                started_rx.recv().await,
                Some(endpoint::postgres("postgres://localhost/current"))
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
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert_eq!(dropped.load(Ordering::SeqCst), 1);
            wait_for_no_action(&mut action_rx).await;
        }
    }
}
