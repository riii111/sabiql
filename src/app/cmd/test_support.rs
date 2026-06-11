use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::cache::TtlCache;
use crate::cmd::runner::{
    ConnectionDeps, EffectRunner, ErDeps, QueryDeps, SettingsDeps, UtilityDeps,
};
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use crate::domain::query_history::QueryHistoryEntry;
use crate::domain::{ConnectionId, DatabaseMetadata, ErTableInfo, QueryResult, QuerySource};
use crate::ports::outbound::{
    ClipboardError, ClipboardWriter, ConfigWriter, ConfigWriterError, ConnectionStore, DsnBuilder,
    ErDiagramExporter, ErExportResult, ErLogWriter, FolderOpenError, FolderOpener,
    MetadataProvider, PgServiceEntryReader, QueryExecutor, QueryHistoryError, QueryHistoryStore,
    ServiceFileError, SettingsStore, SettingsStoreError,
};
use crate::update::action::Action;

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
        _connection_id: &ConnectionId,
        _entry: &QueryHistoryEntry,
    ) -> Result<(), QueryHistoryError> {
        Ok(())
    }

    async fn load(
        &self,
        _project_name: &str,
        _connection_id: &ConnectionId,
    ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
        Ok(Vec::new())
    }
}

pub struct NoopSettingsStore;
impl SettingsStore for NoopSettingsStore {
    fn load(&self) -> Result<crate::ports::outbound::AppSettings, SettingsStoreError> {
        Ok(crate::ports::outbound::AppSettings::default())
    }

    fn save(
        &self,
        _settings: crate::ports::outbound::AppSettings,
    ) -> Result<(), SettingsStoreError> {
        Ok(())
    }
}

pub fn make_runner(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
) -> EffectRunner {
    make_runner_with_dsn(
        metadata_provider,
        query_executor,
        connection_store,
        cache,
        action_tx,
        Arc::new(NoopDsnBuilder),
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
    EffectRunner::new(
        metadata_provider,
        ConnectionDeps {
            dsn_builder,
            connection_store,
            pg_service_entry_reader: Some(Arc::new(NoopPgServiceEntryReader)),
        },
        QueryDeps {
            query_executor,
            query_history_store: Arc::new(NoopQueryHistoryStore),
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
    DatabaseMetadata {
        database_name: "testdb".to_string(),
        schemas: vec![],
        table_summaries: vec![],
    }
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
