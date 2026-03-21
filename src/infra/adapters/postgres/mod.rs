use async_trait::async_trait;

use crate::app::ports::{DbOperationError, MetadataProvider, QueryExecutor};
use crate::domain::{
    Column, DatabaseMetadata, QueryResult, QuerySource, Table, TableSignature, WriteExecutionResult,
};

mod dsn;
mod psql;
mod sql;

pub struct PostgresAdapter {
    timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PostgresAdapter {
    fn extract_primary_key(columns: &[Column]) -> Option<Vec<String>> {
        let pk_cols: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        if pk_cols.is_empty() {
            None
        } else {
            Some(pk_cols)
        }
    }
}

#[async_trait]
impl MetadataProvider for PostgresAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let schemas_json = self.execute_query(dsn, Self::schemas_query()).await?;
        let tables_json = self.execute_query(dsn, Self::tables_query()).await?;

        let schemas = Self::parse_schemas(&schemas_json)?;
        let tables = Self::parse_tables(&tables_json)?;

        let db_name = Self::extract_database_name(dsn);
        let mut metadata = DatabaseMetadata::new(db_name);
        metadata.schemas = schemas;
        metadata.table_summaries = tables;

        Ok(metadata)
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let json = self
            .execute_query(dsn, Self::table_signatures_query())
            .await?;
        Self::parse_table_signatures(&json)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let query = Self::table_detail_query(schema, table);
        let json = self.execute_query(dsn, &query).await?;
        let (columns, indexes, foreign_keys, rls, triggers, table_info) =
            Self::parse_table_detail_combined(&json)?;
        let primary_key = Self::extract_primary_key(&columns);

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            owner: table_info.owner,
            columns,
            primary_key,
            foreign_keys,
            indexes,
            rls,
            triggers,
            row_count_estimate: table_info.row_count_estimate,
            comment: table_info.comment,
        })
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let query = Self::table_columns_and_fks_query(schema, table);
        let json = self.execute_query(dsn, &query).await?;
        let (columns, foreign_keys) = Self::parse_table_columns_and_fks(&json)?;
        let primary_key = Self::extract_primary_key(&columns);

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            owner: None,
            columns,
            primary_key,
            foreign_keys,
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
        })
    }
}

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
        let order_columns = match self.fetch_preview_order_columns(dsn, schema, table).await {
            Ok(cols) => cols,
            Err(e) => {
                eprintln!(
                    "warn: failed to fetch PK columns for {}.{}: {}",
                    schema, table, e
                );
                Vec::new()
            }
        };
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
