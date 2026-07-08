use std::path::Path;

use async_trait::async_trait;

use crate::domain::QueryValue;

use super::DbOperationError;

#[async_trait]
pub trait CachedResultExporter: Send + Sync {
    async fn export_cached_result_to_csv(
        &self,
        path: &Path,
        columns: &[String],
        values: &[Vec<QueryValue>],
    ) -> Result<usize, DbOperationError>;
}
