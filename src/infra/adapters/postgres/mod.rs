use async_trait::async_trait;

use crate::app::ports::{MetadataError, MetadataProvider, QueryExecutor};
use crate::domain::{DatabaseMetadata, QueryResult, QuerySource, Table, WriteExecutionResult};

mod dsn;
mod psql;
mod select_guard;
mod sql;

pub struct PostgresAdapter {
    timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for PostgresAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, MetadataError> {
        let schemas_json = self.execute_query(dsn, Self::schemas_query()).await?;
        let tables_json = self.execute_query(dsn, Self::tables_query()).await?;

        let schemas = Self::parse_schemas(&schemas_json)?;
        let tables = Self::parse_tables(&tables_json)?;

        let db_name = Self::extract_database_name(dsn);
        let mut metadata = DatabaseMetadata::new(db_name);
        metadata.schemas = schemas;
        metadata.tables = tables;

        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError> {
        let columns_q = Self::columns_query(schema, table);
        let indexes_q = Self::indexes_query(schema, table);
        let fks_q = Self::foreign_keys_query(schema, table);
        let rls_q = Self::rls_query(schema, table);
        let triggers_q = Self::triggers_query(schema, table);
        let table_info_q = Self::table_info_query(schema, table);

        // Execute queries sequentially to avoid connection pool exhaustion
        // on tables with many columns
        // TODO: If performance becomes an issue, consider migrating to controlled parallel
        // execution using semaphores (e.g., tokio::sync::Semaphore) to limit concurrency
        let columns_json = self.execute_query(dsn, &columns_q).await?;
        let indexes_json = self.execute_query(dsn, &indexes_q).await?;
        let fks_json = self.execute_query(dsn, &fks_q).await?;
        let rls_json = self.execute_query(dsn, &rls_q).await?;
        let triggers_json = self.execute_query(dsn, &triggers_q).await?;
        let table_info_json = self.execute_query(dsn, &table_info_q).await?;

        let columns = Self::parse_columns(&columns_json)?;
        let indexes = Self::parse_indexes(&indexes_json)?;
        let foreign_keys = Self::parse_foreign_keys(&fks_json)?;
        let rls = Self::parse_rls(&rls_json)?;
        let triggers = Self::parse_triggers(&triggers_json)?;
        let table_info = Self::parse_table_info(&table_info_json)?;

        let pk_cols: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        let primary_key = if pk_cols.is_empty() {
            None
        } else {
            Some(pk_cols)
        };

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
    ) -> Result<QueryResult, MetadataError> {
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
        self.execute_query_raw(dsn, &query, QuerySource::Preview)
            .await
    }

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError> {
        if !select_guard::is_select_query(query) {
            return Err(MetadataError::QueryFailed(
                "Only SELECT queries are supported in SQL modal. Use psql/mycli for DDL/DML operations.".to_string()
            ));
        }

        self.execute_query_raw(dsn, query, QuerySource::Adhoc).await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<WriteExecutionResult, MetadataError> {
        self.execute_write_raw(dsn, query).await
    }
}
