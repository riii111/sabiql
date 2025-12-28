use async_trait::async_trait;

use crate::domain::{DatabaseMetadata, Table};

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, MetadataError>;

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError>;
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MetadataError {
    ConnectionFailed(String),
    QueryFailed(String),
    ParseError(String),
    InvalidJson(String),
    CommandNotFound(String),
    Timeout,
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            Self::QueryFailed(msg) => write!(f, "Query failed: {}", msg),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::InvalidJson(msg) => write!(f, "Invalid JSON: {}", msg),
            Self::CommandNotFound(cmd) => write!(f, "Command not found: {}", cmd),
            Self::Timeout => write!(f, "Operation timed out"),
        }
    }
}

impl std::error::Error for MetadataError {}
