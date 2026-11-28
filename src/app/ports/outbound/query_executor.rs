use async_trait::async_trait;

use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{QueryResult, WriteExecutionResult};

use super::{AccessMode, DbOperationError};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait QueryExecutor: Send + Sync {
    async fn execute_preview(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError>;

    async fn execute_adhoc(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError>;

    async fn execute_write(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError>;
}
