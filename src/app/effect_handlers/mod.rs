pub(crate) mod completion;
pub(crate) mod connection;
pub(crate) mod er;
pub(crate) mod metadata;
pub(crate) mod query;

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::cache::TtlCache;
use crate::app::ports::{
    ConfigWriter, ConnectionStore, DsnBuilder, ErDiagramExporter, ErLogWriter, MetadataProvider,
    QueryExecutor, ServiceFileReader,
};
use crate::domain::DatabaseMetadata;

pub(crate) struct EffectContext<'a> {
    pub metadata_provider: &'a Arc<dyn MetadataProvider>,
    pub query_executor: &'a Arc<dyn QueryExecutor>,
    pub dsn_builder: &'a Arc<dyn DsnBuilder>,
    pub er_exporter: &'a Arc<dyn ErDiagramExporter>,
    pub config_writer: &'a Arc<dyn ConfigWriter>,
    pub er_log_writer: &'a Arc<dyn ErLogWriter>,
    pub connection_store: &'a Arc<dyn ConnectionStore>,
    pub service_file_reader: &'a Arc<dyn ServiceFileReader>,
    pub metadata_cache: &'a TtlCache<String, DatabaseMetadata>,
    pub action_tx: &'a mpsc::Sender<Action>,
}

#[cfg(test)]
pub(crate) mod test_support;
