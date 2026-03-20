use async_trait::async_trait;

use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;

#[derive(Debug, thiserror::Error)]
pub enum QueryHistoryError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Task join error: {0}")]
    JoinError(String),
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
