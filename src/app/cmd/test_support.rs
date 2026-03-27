use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::app::cmd::cache::TtlCache;
use crate::app::cmd::runner::EffectRunner;
use crate::app::ports::{
    ClipboardError, ClipboardWriter, ConfigWriter, ConnectionStore, DsnBuilder, ErDiagramExporter,
    ErExportResult, ErLogWriter, FolderOpenError, FolderOpener, MetadataProvider, QueryExecutor,
    QueryHistoryError, QueryHistoryStore, ServiceFileError, ServiceFileReader,
};
use crate::app::update::action::Action;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use crate::domain::query_history::QueryHistoryEntry;
use crate::domain::{ConnectionId, DatabaseMetadata, ErTableInfo, QueryResult, QuerySource};

pub struct NoopConfigWriter;
impl ConfigWriter for NoopConfigWriter {
    fn get_cache_dir(&self, _project_name: &str) -> color_eyre::eyre::Result<PathBuf> {
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

pub struct NoopServiceFileReader;
impl ServiceFileReader for NoopServiceFileReader {
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

pub fn make_runner(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn ConnectionStore>,
    cache: TtlCache<String, Arc<DatabaseMetadata>>,
    action_tx: mpsc::Sender<Action>,
) -> EffectRunner {
    EffectRunner::builder()
        .metadata_provider(metadata_provider)
        .query_executor(query_executor)
        .dsn_builder(Arc::new(NoopDsnBuilder))
        .er_exporter(Arc::new(NoopErExporter))
        .config_writer(Arc::new(NoopConfigWriter))
        .er_log_writer(Arc::new(NoopErLogWriter))
        .connection_store(connection_store)
        .service_file_reader(Arc::new(NoopServiceFileReader))
        .clipboard(Arc::new(NoopClipboardWriter))
        .folder_opener(Arc::new(NoopFolderOpener))
        .query_history_store(Arc::new(NoopQueryHistoryStore))
        .metadata_cache(cache)
        .action_tx(action_tx)
        .build()
}

pub fn sample_metadata() -> DatabaseMetadata {
    DatabaseMetadata {
        database_name: "testdb".to_string(),
        schemas: vec![],
        table_summaries: vec![],
        fetched_at: Instant::now(),
    }
}

pub fn sample_query_result() -> QueryResult {
    QueryResult {
        query: "SELECT 1".to_string(),
        columns: vec!["id".to_string()],
        rows: vec![vec!["1".to_string()]],
        row_count: 1,
        execution_time_ms: 5,
        executed_at: Instant::now(),
        source: QuerySource::Preview,
        error: None,
        command_tag: None,
    }
}
