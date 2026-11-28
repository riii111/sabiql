use crate::domain::connection::SqliteConnectionConfig;
mod catalog;
mod kind_info;
mod preview;
mod signature;
mod table_detail;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{DatabaseMetadata, Table, TableSignatureSnapshot};

use super::sqlite3::metadata::RawNamedTableMetadata;
use super::{SqliteAdapter, schema::MAIN_SCHEMA};

use catalog::metadata_from_catalog;
use table_detail::{TableDetailMode, table_from_metadata};

fn sqlite_table_not_found(table: &str) -> DbOperationError {
    DbOperationError::ObjectMissing(format!("SQLite table not found: {table}"))
}

impl SqliteAdapter {
    pub async fn fetch_metadata(
        &self,
        config: &SqliteConnectionConfig,
    ) -> Result<DatabaseMetadata, DbOperationError> {
        let path = config.path();
        let tables = self.fetch_catalog_rows(path).await?;
        Ok(metadata_from_catalog(path, &tables))
    }

    pub async fn fetch_table_detail(
        &self,
        config: &SqliteConnectionConfig,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
        table_detail::fetch_table_detail(self, config.path(), table, TableDetailMode::Full).await
    }

    pub async fn fetch_table_columns_and_fks(
        &self,
        config: &SqliteConnectionConfig,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
        table_detail::fetch_table_detail(self, config.path(), table, TableDetailMode::ColumnsAndFks)
            .await
    }

    pub async fn fetch_table_signatures(
        &self,
        config: &SqliteConnectionConfig,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        let rows = self.fetch_table_signature_rows(config.path()).await?;
        let mut signatures = Vec::with_capacity(rows.len());
        let mut prefetched_table_details = Vec::with_capacity(rows.len());
        for RawNamedTableMetadata { name, metadata } in rows {
            let detail = table_from_metadata(&name, TableDetailMode::Signature, metadata)?;
            signatures.push(signature::signature_for_table(&detail));
            prefetched_table_details.push(detail);
        }
        Ok(TableSignatureSnapshot {
            signatures,
            prefetched_table_details,
        })
    }
}

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) fn validate_main_schema(
        schema: &str,
    ) -> Result<(), DbOperationError> {
        if schema == MAIN_SCHEMA {
            Ok(())
        } else {
            Err(DbOperationError::ObjectMissing(format!(
                "SQLite schema not found: {schema}"
            )))
        }
    }
}
