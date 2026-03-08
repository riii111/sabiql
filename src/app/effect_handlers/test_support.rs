use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::cache::TtlCache;
use crate::app::effect_runner::EffectRunner;
use crate::app::ports::{
    ConfigWriter, DsnBuilder, ErDiagramExporter, ErExportResult, ErLogWriter, MetadataProvider,
    QueryExecutor, ServiceFileReader,
};
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, QueryResult, QuerySource};

pub(crate) struct NoopConfigWriter;
impl ConfigWriter for NoopConfigWriter {
    fn get_cache_dir(&self, _project_name: &str) -> color_eyre::eyre::Result<PathBuf> {
        Ok(PathBuf::from("/tmp"))
    }
}

pub(crate) struct NoopErExporter;
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

pub(crate) struct NoopErLogWriter;
impl ErLogWriter for NoopErLogWriter {
    fn write_er_failure_log(
        &self,
        _failed_tables: Vec<(String, String)>,
        _cache_dir: PathBuf,
    ) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) struct NoopDsnBuilder;
impl DsnBuilder for NoopDsnBuilder {
    fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
        String::new()
    }
}

pub(crate) struct NoopServiceFileReader;
impl ServiceFileReader for NoopServiceFileReader {
    fn read_services(
        &self,
    ) -> Result<
        (
            Vec<crate::domain::connection::ServiceEntry>,
            std::path::PathBuf,
        ),
        crate::app::ports::ServiceFileError,
    > {
        Ok((vec![], std::path::PathBuf::new()))
    }
}

pub(crate) fn make_runner(
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    connection_store: Arc<dyn crate::app::ports::ConnectionStore>,
    cache: TtlCache<String, DatabaseMetadata>,
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
        .metadata_cache(cache)
        .action_tx(action_tx)
        .build()
}

pub(crate) fn sample_metadata() -> DatabaseMetadata {
    DatabaseMetadata {
        database_name: "testdb".to_string(),
        schemas: vec![],
        tables: vec![],
        fetched_at: Instant::now(),
    }
}

pub(crate) fn sample_query_result() -> QueryResult {
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
