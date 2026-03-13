use async_trait::async_trait;

use crate::domain::connection::ConnectionId;
use crate::domain::query_history::QueryHistoryEntry;

#[async_trait]
pub trait QueryHistoryStore: Send + Sync {
    async fn append(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
        entry: &QueryHistoryEntry,
    ) -> Result<(), String>;

    async fn load(
        &self,
        project_name: &str,
        connection_id: &ConnectionId,
    ) -> Result<Vec<QueryHistoryEntry>, String>;
}
