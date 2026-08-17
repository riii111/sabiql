use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::cache::TtlCache;
use crate::cmd::runner::{
    ConnectionDeps, EffectRunner, ErDeps, QueryDeps, SettingsDeps, UtilityDeps,
};
use crate::domain::SqliteDiagnosticsSnapshot;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, ErTableInfo, QueryResult, QuerySource, QueryValue,
    SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error,
    mysql_sql::MySqlStatement,
};
use crate::ports::outbound::DbOperationError;
use crate::ports::outbound::{
    AccessMode, AppSettings, CachedResultExporter, ClipboardError, ClipboardWriter, ConfigWriter,
    ConfigWriterError, ConnectionStore, DsnBuilder, ErDiagramExporter, ErExportResult, ErLogWriter,
    FolderOpenError, FolderOpener, MetadataProvider, MySqlConnectionProbe, MySqlQueryExecutor,
    PgServiceEntryReader, QueryExecutor, QueryHistoryError, QueryHistoryStore, ServiceFileError,
    SettingsStore, SettingsStoreError, SqliteDiagnosticsProvider, SqlitePathValidator,
};
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
    async fn probe(&self, _dsn: &str) -> Result<(), DbOperationError> {
        Ok(())
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
    fn open(&self, _path: &Path) -> Result<(), FolderOpenError> {
        Ok(())
    }
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

pub struct NoopMySqlQueryExecutor;
#[async_trait::async_trait]
impl MySqlQueryExecutor for NoopMySqlQueryExecutor {
    async fn execute_adhoc_with_classified_statements(
        &self,
        _dsn: &str,
        _query: &str,
        _statements: &[MySqlStatement],
        _access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        Err(DbOperationError::UnsupportedOperation(
            "classified MySQL statements require the MySQL query executor".to_string(),
        ))
    }
}

pub struct NoopSettingsStore;
impl SettingsStore for NoopSettingsStore {
    fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        Ok(AppSettings::default())
    }

    fn save(&self, _settings: AppSettings) -> Result<(), SettingsStoreError> {
        Ok(())
    }
}

pub struct NoopSqliteDiagnosticsProvider;
#[async_trait::async_trait]
impl SqliteDiagnosticsProvider for NoopSqliteDiagnosticsProvider {
    async fn fetch_diagnostics_core(
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
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
) -> EffectRunner {
    make_runner_with_mysql_query_executor(
        metadata_provider,
        query_executor,
        Arc::new(NoopMySqlQueryExecutor),
        connection_store,
        cache,
        action_tx,
    )
}

pub fn make_runner_with_mysql_query_executor(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    mysql_query_executor: Arc<dyn MySqlQueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        Arc::new(NoopDsnBuilder),
        Arc::new(NoopMySqlConnectionProbe),
        Arc::new(TestCachedResultExporter),
        mysql_query_executor,
    )
}

pub fn make_runner_with_cached_result_exporter(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    cached_result_exporter: Arc<dyn CachedResultExporter>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        Arc::new(NoopDsnBuilder),
        cached_result_exporter,
    )
}

pub fn make_runner_with_dsn(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
) -> EffectRunner {
    make_runner_with_dsn_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        dsn_builder,
        Arc::new(NoopMySqlConnectionProbe),
    )
}

pub fn make_runner_with_dsn_and_probe(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
    mysql_connection_probe: Arc<dyn MySqlConnectionProbe>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        dsn_builder,
        mysql_connection_probe,
        Arc::new(TestCachedResultExporter),
        Arc::new(NoopMySqlQueryExecutor),
    )
}

fn make_runner_with_dsn_and_cached_result_exporter(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
    cached_result_exporter: Arc<dyn CachedResultExporter>,
) -> EffectRunner {
    make_runner_with_dsn_and_cached_result_exporter_and_probe(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        dsn_builder,
        Arc::new(NoopMySqlConnectionProbe),
        cached_result_exporter,
        Arc::new(NoopMySqlQueryExecutor),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "test fixture wires independent effect dependencies"
)]
fn make_runner_with_dsn_and_cached_result_exporter_and_probe(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
    dsn_builder: Arc<dyn DsnBuilder>,
    mysql_connection_probe: Arc<dyn MySqlConnectionProbe>,
    cached_result_exporter: Arc<dyn CachedResultExporter>,
    mysql_query_executor: Arc<dyn MySqlQueryExecutor>,
) -> EffectRunner {
    EffectRunner::new(
        metadata_provider,
        ConnectionDeps {
            dsn_builder,
            mysql_connection_probe,
            connection_store,
            pg_service_entry_reader: Some(Arc::new(NoopPgServiceEntryReader)),
            sqlite_path_validator: Arc::new(TestFsSqlitePathValidator),
        },
        QueryDeps {
            query_executor,
            mysql_query_executor,
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
        SettingsDeps {
            settings_store: Arc::new(NoopSettingsStore),
        },
        cache,
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
