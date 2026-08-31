use serde::Deserialize;

use crate::app::ports::outbound::DbOperationError;

use super::super::{SqliteAdapter, sql};
use super::error::validate_safe_mode_version;

mod raw;

pub(in crate::adapters::sqlite) use raw::{
    RawBatchIndex, RawColumn, RawForeignKey, RawIndexColumn, RawNamedTableMetadata,
    RawPreviewMetadata, RawReferencedColumns, RawTable, RawTableKindInfo, RawTableMetadata,
};
use raw::{RawCatalog, RawJsonPayload, RawNamedJsonPayload};

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) async fn fetch_catalog_rows(
        &self,
        path: &str,
    ) -> Result<Vec<RawTable>, DbOperationError> {
        self.execute_catalog_query(path, sql::user_tables_query())
            .await
    }

    pub(in crate::adapters::sqlite) async fn fetch_preview_metadata_rows(
        &self,
        path: &str,
        table: &str,
    ) -> Result<RawPreviewMetadata, DbOperationError> {
        self.execute_json_payload(path, &sql::preview_metadata_query(table))
            .await
    }

    pub(in crate::adapters::sqlite) async fn fetch_table_metadata_rows(
        &self,
        path: &str,
        table: &str,
        mode: sql::TableMetadataQueryMode,
    ) -> Result<RawTableMetadata, DbOperationError> {
        let metadata = self
            .execute_json_payload(path, &sql::table_metadata_query(table, mode))
            .await;
        if let Some(fallback_mode) = row_count_fallback_mode(mode, &metadata) {
            return self
                .execute_json_payload(path, &sql::table_metadata_query(table, fallback_mode))
                .await;
        }
        metadata
    }

    pub(in crate::adapters::sqlite) async fn fetch_table_signature_rows(
        &self,
        path: &str,
    ) -> Result<Vec<RawNamedTableMetadata>, DbOperationError> {
        let rows: Vec<RawNamedJsonPayload> = self
            .cli
            .execute_json_read_only(path, &sql::table_signatures_query())
            .await?;
        rows.into_iter()
            .map(|row| {
                let metadata =
                    serde_json::from_str(&row.payload).map_err(DbOperationError::from)?;
                Ok(RawNamedTableMetadata {
                    name: row.name,
                    metadata,
                })
            })
            .collect()
    }

    async fn execute_json_payload<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &str,
    ) -> Result<T, DbOperationError> {
        let rows: Vec<RawJsonPayload> = self.cli.execute_json_read_only(path, query).await?;
        let payload = rows.into_iter().next().ok_or_else(|| {
            DbOperationError::MetadataParseFailed("SQLite metadata payload was empty".to_string())
        })?;
        serde_json::from_str(&payload.payload).map_err(DbOperationError::from)
    }

    async fn execute_catalog_query(
        &self,
        path: &str,
        query: &str,
    ) -> Result<Vec<RawTable>, DbOperationError> {
        let catalog: RawCatalog = self.execute_json_payload(path, query).await?;
        validate_safe_mode_version(&catalog.sqlite_version)?;
        Ok(catalog.tables)
    }
}

fn row_count_fallback_mode<T>(
    mode: sql::TableMetadataQueryMode,
    result: &Result<T, DbOperationError>,
) -> Option<sql::TableMetadataQueryMode> {
    result.as_ref().err()?;
    mode.without_row_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_timeout_uses_row_count_fallback() {
        let result: Result<(), DbOperationError> = Err(DbOperationError::LockTimeout(
            "database is locked".to_string(),
        ));

        assert_eq!(
            row_count_fallback_mode(sql::TableMetadataQueryMode::Full, &result),
            Some(sql::TableMetadataQueryMode::FullWithoutRowCount)
        );
    }
}
