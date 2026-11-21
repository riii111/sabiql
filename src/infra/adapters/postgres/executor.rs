use crate::adapters::csv_export::export_to_downloads;
use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::connection::PostgresEndpoint;
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};

use super::PostgresAdapter;
use super::dsn::postgres_conninfo;

impl PostgresAdapter {
    pub async fn execute_preview(
        &self,
        endpoint: &PostgresEndpoint,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        // Editing a cell re-fetches the same page; stable ordering prevents the
        // edited row from shifting position after the refresh.
        // On failure, falls back to unordered preview (rows may shift after edits).
        let dsn = postgres_conninfo(endpoint);
        let order_columns = self
            .fetch_preview_order_columns(&dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_result(&dsn, &query, QuerySource::Preview, true)
            .await
    }

    pub async fn execute_adhoc(
        &self,
        endpoint: &PostgresEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        self.execute_query_result(
            &postgres_conninfo(endpoint),
            query,
            QuerySource::Adhoc,
            access_mode.is_read_only(),
        )
        .await
    }

    pub async fn execute_write(
        &self,
        endpoint: &PostgresEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        self.execute_write_raw(
            &postgres_conninfo(endpoint),
            query,
            access_mode.is_read_only(),
        )
        .await
    }

    pub async fn export_to_csv(
        &self,
        endpoint: &PostgresEndpoint,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        let dsn = postgres_conninfo(endpoint);
        export_to_downloads(file_name, |path| async move {
            self.export_csv_to_file(&dsn, query, &path, true).await
        })
        .await
    }
}
