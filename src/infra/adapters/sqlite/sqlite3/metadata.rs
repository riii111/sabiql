use serde::Deserialize;

use crate::app::ports::outbound::DbOperationError;

use super::super::{SqliteAdapter, sql};

mod raw;

pub(in crate::adapters::sqlite) use raw::{
    RawBatchIndex, RawColumn, RawForeignKey, RawIndexColumn, RawNamedTableMetadata,
    RawPreviewMetadata, RawReferencedColumns, RawTable, RawTableKindInfo, RawTableMetadata,
};
use raw::{RawJsonPayload, RawNamedJsonPayload, RawRowCount};

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) async fn fetch_catalog_rows(
        &self,
        path: &str,
    ) -> Result<Vec<RawTable>, DbOperationError> {
        match self.cli.execute_json(path, sql::user_tables_query()).await {
            Ok(tables) => Ok(tables),
            Err(DbOperationError::QueryFailed(message))
                if sql::is_table_list_unavailable(&message) =>
            {
                if self.has_virtual_tables(path).await? {
                    return Err(sql::table_list_required_error());
                }
                self.cli
                    .execute_json(path, sql::legacy_user_tables_query())
                    .await
            }
            Err(error) => Err(error),
        }
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
            .execute_json(path, &sql::table_signatures_query())
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

    async fn has_virtual_tables(&self, path: &str) -> Result<bool, DbOperationError> {
        let rows: Vec<RawRowCount> = self
            .cli
            .execute_json(path, sql::has_virtual_tables_query())
            .await?;
        Ok(rows.into_iter().next().is_some_and(|row| row.count > 0))
    }

    async fn execute_json_payload<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &str,
    ) -> Result<T, DbOperationError> {
        let rows: Vec<RawJsonPayload> = self.cli.execute_json(path, query).await?;
        let payload = rows.into_iter().next().ok_or_else(|| {
            DbOperationError::MetadataParseFailed("SQLite metadata payload was empty".to_string())
        })?;
        serde_json::from_str(&payload.payload).map_err(DbOperationError::from)
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
    use crate::adapters::test_support;

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

    #[tokio::test]
    async fn detects_virtual_tables_in_schema() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE VIRTUAL TABLE notes_fts USING fts5(body);");
        let adapter = SqliteAdapter::new();
        let path = SqliteAdapter::path_from_dsn(&dsn).unwrap();

        assert!(adapter.has_virtual_tables(path).await.unwrap());
    }

    #[tokio::test]
    async fn simple_schema_has_no_virtual_tables() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();
        let path = SqliteAdapter::path_from_dsn(&dsn).unwrap();

        assert!(!adapter.has_virtual_tables(path).await.unwrap());
    }
}
