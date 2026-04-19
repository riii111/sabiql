use async_trait::async_trait;

use crate::domain::{DatabaseMetadata, Table, TableSignature};

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError>;

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError>;

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError>;

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError>;
}
