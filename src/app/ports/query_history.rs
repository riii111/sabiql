use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;

#[derive(Debug, Clone, thiserror::Error)]
pub enum QueryHistoryError {
    #[error("IO error: {0}")]
    IoError(Arc<std::io::Error>),
    #[error("Serialization error: {0}")]
    SerializationError(Arc<serde_json::Error>),
    #[error("Task join error: {0}")]
    JoinError(Arc<tokio::task::JoinError>),
}

impl From<std::io::Error> for QueryHistoryError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(Arc::new(e))
    }
}

impl From<serde_json::Error> for QueryHistoryError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError(Arc::new(e))
    }
}

impl From<tokio::task::JoinError> for QueryHistoryError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::JoinError(Arc::new(e))
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
