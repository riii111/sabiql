use async_trait::async_trait;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::ports::outbound::{AccessMode, DbOperationError, QueryExecutor};
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
    ) -> Result<QueryResult, DbOperationError> {
        // Editing a cell re-fetches the same page; stable ordering prevents the
        // edited row from shifting position after the refresh.
        // On failure, falls back to unordered preview (rows may shift after edits).
        let order_columns = self
            .fetch_preview_order_columns(dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_raw(dsn, &query, QuerySource::Preview, true)
            .await
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        self.execute_query_raw(dsn, query, QuerySource::Adhoc, access_mode.is_read_only())
            .await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        self.execute_write_raw(dsn, query, access_mode.is_read_only())
            .await
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
        self.count_rows(dsn, query, true).await
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        export_to_downloads(file_name, |path| async move {
            self.export_csv_to_file(dsn, query, &path, true)
                .await
                .map(|_| ())
        })
        .await
    }
}
