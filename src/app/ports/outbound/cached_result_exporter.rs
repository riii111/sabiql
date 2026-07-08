use std::path::PathBuf;

use async_trait::async_trait;

use crate::domain::QueryValue;

use super::DbOperationError;

#[async_trait]
pub trait CachedResultExporter: Send + Sync {
    async fn export_cached_result_to_csv(
        &self,
        path: PathBuf,
        columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
    ) -> Result<usize, DbOperationError>;
}
