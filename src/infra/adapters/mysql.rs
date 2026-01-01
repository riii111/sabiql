use async_trait::async_trait;

use crate::app::ports::{MetadataError, MetadataProvider, QueryExecutor};
use crate::domain::{DatabaseMetadata, QueryResult, Table};

#[allow(dead_code)]
pub struct MySqlAdapter;

#[allow(dead_code)]
impl MySqlAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, _dsn: &str) -> Result<DatabaseMetadata, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn fetch_table_detail(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
    ) -> Result<Table, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }
}

#[async_trait]
impl QueryExecutor for MySqlAdapter {
    async fn execute_preview(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
        _limit: usize,
    ) -> Result<QueryResult, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn execute_adhoc(&self, _dsn: &str, _query: &str) -> Result<QueryResult, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }
}
