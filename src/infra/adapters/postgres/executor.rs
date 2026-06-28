use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, QueryExecutor};
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};

use super::PostgresAdapter;

#[async_trait]
impl QueryExecutor for PostgresAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        // Editing a cell re-fetches the same page; stable ordering prevents the
        // edited row from shifting position after the refresh.
        // On failure, falls back to unordered preview (rows may shift after edits).
        let order_columns = self
            .fetch_preview_order_columns(dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_raw(dsn, &query, QuerySource::Preview, read_only)
            .await
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        self.execute_query_raw(dsn, query, QuerySource::Adhoc, read_only)
            .await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        self.execute_write_raw(dsn, query, read_only).await
    }

    async fn count_query_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        self.count_rows(dsn, query, read_only).await
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        self.export_csv_to_file(dsn, query, path, read_only).await
    }
}
