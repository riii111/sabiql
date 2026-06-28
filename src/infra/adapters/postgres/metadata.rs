use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{Column, DatabaseMetadata, Table, TableSignature, TableStorage};

use super::PostgresAdapter;

fn extract_primary_key(columns: &[Column]) -> Option<Vec<String>> {
    let pk_cols: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key())
        .map(|c| c.name.clone())
        .collect();
    if pk_cols.is_empty() {
        None
    } else {
        Some(pk_cols)
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
        let primary_key = extract_primary_key(&columns);

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
            source_ddl: None,
            storage: TableStorage::default(),
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
        let primary_key = extract_primary_key(&columns);

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
            source_ddl: None,
            storage: TableStorage::default(),
        })
    }
}
