use async_trait::async_trait;

use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{DatabaseMetadata, Table, TableSignatureSnapshot};

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn fetch_metadata(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<DatabaseMetadata, DbOperationError>;

    async fn fetch_effective_user(
        &self,
        _endpoint: &ConnectionEndpoint,
    ) -> Result<Option<String>, DbOperationError> {
        Ok(None)
    }

    async fn fetch_table_detail(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError>;

    async fn fetch_table_columns_and_fks(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError>;

    async fn fetch_table_signatures(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<TableSignatureSnapshot, DbOperationError>;
}
