use async_trait::async_trait;

use crate::domain::{DatabaseMetadata, Table, TableSignature};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, MetadataError>;

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError>;

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError>;

    async fn fetch_table_signatures(&self, dsn: &str)
    -> Result<Vec<TableSignature>, MetadataError>;
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MetadataError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("Operation timed out")]
    Timeout,
}
