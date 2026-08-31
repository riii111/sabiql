use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::effect::Effect;
use crate::cmd::runner::{ConnectionDeps, EffectRunner, ErDeps, QueryDeps, UtilityDeps};
use crate::domain::SqliteDiagnosticsSnapshot;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, ErTableInfo, QueryResult, QuerySource, QueryValue,
    SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error,
};
use crate::model::app_state::AppState;
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::DbOperationError;
use crate::ports::outbound::{
    AppSettings, CachedResultExporter, ClipboardError, ClipboardWriter, ConfigWriter,
    ConfigWriterError, ConnectionStore, DsnBuilder, ErDiagramExporter, ErExportResult, ErLogWriter,
    FolderOpener, MetadataProvider, MySqlConnectionProbe, MySqlConnectionProbeResult,
    PgServiceEntryReader, QueryExecutor, QueryHistoryError, QueryHistoryStore, RenderOutput,
    RenderResult, Renderer, ServiceFileError, SettingsStore, SettingsStoreError,
    SqliteDiagnosticsProvider, SqlitePathValidator,
};
use crate::services::AppServices;
use crate::update::action::Action;

#[derive(Debug, Default, Clone, Copy)]
pub struct TestFsSqlitePathValidator;

impl SqlitePathValidator for TestFsSqlitePathValidator {
    fn validate_database_path(&self, path: &str) -> Result<(), SqlitePathError> {
        let path = Path::new(path);
        let display = path.display().to_string();
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(classify_sqlite_metadata_error(
                    &display,
                    error.kind(),
                    &error.to_string(),
                ));
            }
        };

        if metadata.is_dir() {
            return Err(SqlitePathError::IsDirectory(display));
        }

        if !metadata.is_file() {
            return Err(SqlitePathError::NotRegularFile(display));
        }

        match std::fs::File::open(path) {
            Ok(_) => Ok(()),
            Err(error) => Err(classify_sqlite_read_error(
                &display,
                error.kind(),
                &error.to_string(),
            )),
        }
    }

    fn canonicalize_database_path(&self, path: &str) -> Result<PathBuf, SqlitePathError> {
        let path = Path::new(path);
        let display = path.display().to_string();
        std::fs::canonicalize(path).map_err(|error| {
            classify_sqlite_metadata_error(&display, error.kind(), &error.to_string())
        })
    }
}

pub struct TestCachedResultExporter;
#[async_trait::async_trait]
impl CachedResultExporter for TestCachedResultExporter {
    async fn export_cached_result_to_csv(
        &self,
        file_name: String,
        _columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
    ) -> Result<PathBuf, DbOperationError> {
        Ok(PathBuf::from(format!(
            "/tmp/{file_name}_{}.csv",
            values.len()
        )))
    }
}

pub struct NoopConfigWriter;
impl ConfigWriter for NoopConfigWriter {
    fn get_cache_dir(&self, _project_name: &str) -> Result<PathBuf, ConfigWriterError> {
        Ok(PathBuf::from("/tmp"))
    }
}

pub struct NoopErExporter;
impl ErDiagramExporter for NoopErExporter {
    fn generate_and_export(
        &self,
        _tables: &[ErTableInfo],
        _filename: &str,
        _cache_dir: &Path,
        _browser: Option<&str>,
    ) -> ErExportResult<PathBuf> {
        Ok(PathBuf::from("/tmp/er.svg"))
    }
}

pub struct NoopErLogWriter;
impl ErLogWriter for NoopErLogWriter {
    fn write_er_failure_log(
        &self,
        _failed_tables: Vec<(String, String)>,
        _cache_dir: PathBuf,
    ) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct NoopDsnBuilder;
impl DsnBuilder for NoopDsnBuilder {
    fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
        String::new()
    }
}

pub struct NoopMySqlConnectionProbe;
#[async_trait::async_trait]
impl MySqlConnectionProbe for NoopMySqlConnectionProbe {
    async fn probe(&self, _dsn: &str) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        Ok(MySqlConnectionProbeResult {
            lower_case_table_names: 0,
        })
    }
}

pub struct NoopPgServiceEntryReader;
impl PgServiceEntryReader for NoopPgServiceEntryReader {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError> {
        Ok((vec![], PathBuf::new()))
    }
}

pub struct NoopClipboardWriter;
impl ClipboardWriter for NoopClipboardWriter {
    fn copy_text(&self, _content: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
}

pub struct NoopFolderOpener;
impl FolderOpener for NoopFolderOpener {
    fn open(&self, _path: &Path) -> Result<(), std::io::Error> {
        Ok(())
    }
}

pub struct NoopRenderer;
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

pub fn active_connection_save_guard(run_id: u64) -> Arc<ConnectionSaveGuard> {
    let guard = Arc::new(ConnectionSaveGuard::default());
    guard.arm_save(run_id);
    guard
}

pub struct EffectRun {
    pub actions: Vec<Action>,
}

pub fn run_one_effect<'a>(
    runner: &'a EffectRunner,
    effect: Effect,
    mut state: AppState,
    completion_engine: RefCell<CompletionEngine>,
    action_rx: &'a mut mpsc::Receiver<Action>,
    action_timeout: Option<Duration>,
) -> Pin<Box<dyn Future<Output = Result<EffectRun>> + 'a>> {
    Box::pin(async move {
        let mut renderer = NoopRenderer;
        let mut actions = runner
            .execute_effects(
                vec![effect],
                &mut renderer,
                &mut state,
                &completion_engine,
                &AppServices::stub(),
            )
            .await?;

        if let Some(timeout) = action_timeout {
            actions.push(recv_action_with_timeout(action_rx, timeout).await);
        }

        Ok(EffectRun { actions })
    })
}

pub async fn recv_action_with_timeout(
    rx: &mut mpsc::Receiver<Action>,
    timeout: Duration,
) -> Action {
    tokio::time::timeout(timeout, rx.recv())
        .await
        .expect("action timeout")
        .expect("channel closed")
}

pub struct NoopQueryHistoryStore;
#[async_trait::async_trait]
impl QueryHistoryStore for NoopQueryHistoryStore {
    async fn append(
        &self,
        _project_name: &str,
        _scope: &QueryHistoryScope,
        _entry: &QueryHistoryEntry,
    ) -> Result<(), QueryHistoryError> {
        Ok(())
    }

    async fn load(
        &self,
        _project_name: &str,
        _scope: &QueryHistoryScope,
    ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
        Ok(Vec::new())
    }
}

pub struct NoopSettingsStore;
impl SettingsStore for NoopSettingsStore {
    fn save(&self, _settings: AppSettings) -> Result<(), SettingsStoreError> {
        Ok(())
    }
}

pub struct NoopSqliteDiagnosticsProvider;
#[async_trait::async_trait]
impl SqliteDiagnosticsProvider for NoopSqliteDiagnosticsProvider {
    async fn fetch_core_diagnostics(
        &self,
        _dsn: &str,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        Ok(SqliteDiagnosticsSnapshot::default())
    }

    async fn fetch_quick_check(&self, _dsn: &str) -> DiagnosticField {
        DiagnosticField::default()
    }
}

pub fn make_runner(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    action_tx: mpsc::Sender<Action>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        action_tx,
        Arc::new(NoopDsnBuilder),
        Arc::new(NoopMySqlConnectionProbe),
        Arc::new(TestCachedResultExporter),
    )
}

pub fn make_runner_with_cached_result_exporter(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    action_tx: mpsc::Sender<Action>,
    cached_result_exporter: Arc<dyn CachedResultExporter>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        action_tx,
        Arc::new(NoopDsnBuilder),
        Arc::new(NoopMySqlConnectionProbe),
        cached_result_exporter,
    )
}

pub fn make_runner_with_dsn(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
) -> EffectRunner {
    make_runner_with_dsn_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        action_tx,
        dsn_builder,
        Arc::new(NoopMySqlConnectionProbe),
    )
}

pub fn make_runner_with_dsn_and_probe(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
    mysql_connection_probe: Arc<dyn MySqlConnectionProbe>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        action_tx,
        dsn_builder,
        mysql_connection_probe,
        Arc::new(TestCachedResultExporter),
    )
}

fn make_runner_with_dsn_and_cached_result_exporter_and_probe(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
    mysql_connection_probe: Arc<dyn MySqlConnectionProbe>,
    cached_result_exporter: Arc<dyn CachedResultExporter>,
) -> EffectRunner {
    EffectRunner::new(
        metadata_provider,
        ConnectionDeps {
            dsn_builder,
            mysql_connection_probe,
            connection_store,
            pg_service_entry_reader: Arc::new(NoopPgServiceEntryReader),
            sqlite_path_validator: Arc::new(TestFsSqlitePathValidator),
        },
        QueryDeps {
            query_executor,
            query_history_store: Arc::new(NoopQueryHistoryStore),
            sqlite_diagnostics: Arc::new(NoopSqliteDiagnosticsProvider),
            cached_result_exporter,
        },
        ErDeps {
            er_exporter: Arc::new(NoopErExporter),
            config_writer: Arc::new(NoopConfigWriter),
            er_log_writer: Arc::new(NoopErLogWriter),
        },
        UtilityDeps {
            clipboard: Arc::new(NoopClipboardWriter),
            folder_opener: Arc::new(NoopFolderOpener),
        },
        Arc::new(NoopSettingsStore),
        action_tx,
    )
}

pub fn sample_metadata() -> DatabaseMetadata {
    DatabaseMetadata::new("testdb".to_string())
}

pub fn sample_query_result() -> QueryResult {
    QueryResult::success(
        "SELECT 1".to_string(),
        vec!["id".to_string()],
        vec![vec!["1".to_string()]],
        5,
        QuerySource::Preview,
    )
}
