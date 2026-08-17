use std::path::PathBuf;

use async_trait::async_trait;

use crate::domain::{QueryResult, WriteExecutionResult, mysql_sql::MySqlStatement};

use super::{AccessMode, DbOperationError};

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
    ) -> Result<QueryResult, DbOperationError>;

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError>;

    async fn execute_adhoc_with_classified_mysql_statements(
        &self,
        dsn: &str,
        query: &str,
        statements: &[MySqlStatement],
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        let _ = (dsn, query, statements, access_mode);
        Err(DbOperationError::UnsupportedOperation(
            "classified MySQL statements require the MySQL query executor".to_string(),
        ))
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError>;
    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError>;
    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<PathBuf, DbOperationError>;
}
