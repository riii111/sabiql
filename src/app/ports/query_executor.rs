use async_trait::async_trait;

use crate::domain::{QueryResult, WriteExecutionResult};

use super::MetadataError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait QueryExecutor: Send + Sync {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, MetadataError>;

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError>;
    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<WriteExecutionResult, MetadataError>;
}
