mod catalog;
mod kind_info;
mod preview;
mod signature;
mod table_detail;
mod trigger;

use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{DatabaseMetadata, Table, TableSignature};

use super::sqlite3::metadata::RawNamedTableMetadata;
use super::{SqliteAdapter, schema::MAIN_SCHEMA};

use catalog::metadata_from_catalog;
use table_detail::{TableDetailMode, table_from_metadata};

#[async_trait]
impl MetadataProvider for SqliteAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        self.cli.ensure_safe_mode_supported().await?;
        let path = Self::path_from_dsn(dsn)?;
        let tables = self.fetch_catalog_rows(path).await?;
        Ok(metadata_from_catalog(path, &tables))
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
        table_detail::fetch_table_detail(
            self,
            Self::path_from_dsn(dsn)?,
            table,
            TableDetailMode::Full,
        )
        .await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
        table_detail::fetch_table_detail(
            self,
            Self::path_from_dsn(dsn)?,
            table,
            TableDetailMode::ColumnsAndFks,
        )
        .await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let rows = self
            .fetch_table_signature_rows(Self::path_from_dsn(dsn)?)
            .await?;
        rows.into_iter()
            .map(|RawNamedTableMetadata { name, metadata }| {
                let detail = table_from_metadata(&name, TableDetailMode::Signature, metadata)?;
                Ok(signature::signature_for_table(&detail))
            })
            .collect()
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
