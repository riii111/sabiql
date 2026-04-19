use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;

#[derive(Debug, Clone, thiserror::Error)]
pub enum QueryHistoryError {
    #[error("cache directory is unavailable")]
    MissingCacheDir,
    #[error("IO error: {0}")]
    Io(#[source] Arc<std::io::Error>),
    #[error("Serialization error: {0}")]
    Serialization(#[source] Arc<serde_json::Error>),
    #[error("Task join error: {0}")]
    Join(#[source] Arc<tokio::task::JoinError>),
}

impl From<std::io::Error> for QueryHistoryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(Arc::new(e))
    }
}

impl From<serde_json::Error> for QueryHistoryError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(Arc::new(e))
    }
}

impl From<tokio::task::JoinError> for QueryHistoryError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::Join(Arc::new(e))
    }
}

#[async_trait]
pub trait QueryHistoryStore: Send + Sync {
    async fn append(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
        entry: &QueryHistoryEntry,
    ) -> Result<(), QueryHistoryError>;

    async fn load(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
    ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn missing_cache_dir_has_clear_display() {
        assert_eq!(
            QueryHistoryError::MissingCacheDir.to_string(),
            "cache directory is unavailable"
        );
    }

    #[test]
    fn io_variant_preserves_source_chain() {
        let err: QueryHistoryError = std::io::Error::other("disk full").into();
        assert!(err.source().is_some());
    }
}
