use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::QueryResult;

use super::DbOperationError;

#[async_trait]
pub trait CachedResultExporter: Send + Sync {
    async fn export_cached_result_to_csv(
        &self,
        file_name: String,
        result: Arc<QueryResult>,
    ) -> Result<PathBuf, DbOperationError>;
}
