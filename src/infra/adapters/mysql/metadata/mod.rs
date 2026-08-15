use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{DatabaseMetadata, Schema, Table, TableSignature};

use super::adapter::MySqlAdapter;

mod catalog;
mod preview;
mod signature;
mod table_detail;

pub(super) use preview::{convert_preview_values, fetch_preview_metadata, preview_result_columns};

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let snapshot = catalog::fetch_metadata_snapshot(dsn).await?;
        let mut metadata = DatabaseMetadata::new(snapshot.database.clone());
        metadata.schemas = vec![Schema::new(snapshot.database)];
        metadata.table_summaries = snapshot.table_summaries;
        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_detail_in_session(dsn, schema, table).await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_columns_and_fks(dsn, schema, table).await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        signature::fetch_table_signatures(dsn).await
    }
}
