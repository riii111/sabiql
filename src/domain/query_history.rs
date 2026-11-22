use serde::{Deserialize, Serialize};

use super::connection::ConnectionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHistoryScope {
    pub connection_id: ConnectionId,
    pub database: Option<String>,
}

impl QueryHistoryScope {
    pub fn new(connection_id: ConnectionId, database: Option<String>) -> Self {
        Self {
            connection_id,
            database,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Iso8601Timestamp(String);

impl Iso8601Timestamp {
    pub fn new(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Iso8601Timestamp {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for Iso8601Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryResultStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryHistoryEntry {
    pub query: String,
    pub executed_at: Iso8601Timestamp,
    pub connection_id: ConnectionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub result_status: QueryResultStatus,
    pub affected_rows: Option<u64>,
}

impl QueryHistoryEntry {
    pub fn new_with_database(
        query: String,
        executed_at: String,
        connection_id: ConnectionId,
        database: Option<String>,
        result_status: QueryResultStatus,
        affected_rows: Option<u64>,
    ) -> Self {
        Self {
            query,
            executed_at: Iso8601Timestamp::new(executed_at),
            connection_id,
            database,
            result_status,
            affected_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_update_preserves_database_and_affected_rows() {
        let entry = QueryHistoryEntry::new_with_database(
            "UPDATE users SET active = true".into(),
            "2026-03-13T12:00:00Z".into(),
            ConnectionId::from_string("abc-123"),
            Some("analytics".into()),
            QueryResultStatus::Success,
            Some(5),
        );
        let saved = serde_json::json!({
            "query": "UPDATE users SET active = true",
            "executed_at": "2026-03-13T12:00:00Z",
            "connection_id": "abc-123",
            "database": "analytics",
            "result_status": "Success",
            "affected_rows": 5,
        });

        assert_eq!(serde_json::to_value(&entry).unwrap(), saved);
        assert_eq!(
            serde_json::from_value::<QueryHistoryEntry>(saved).unwrap(),
            entry
        );
    }

    #[test]
    fn legacy_failure_without_database_remains_unscoped() {
        let saved = serde_json::json!({
            "query": "SELECT missing",
            "executed_at": "2026-03-13T12:00:00Z",
            "connection_id": "abc-123",
            "result_status": "Failed",
            "affected_rows": null,
        });

        let entry: QueryHistoryEntry = serde_json::from_value(saved.clone()).unwrap();

        assert_eq!(entry.database, None);
        assert_eq!(entry.result_status, QueryResultStatus::Failed);
        assert_eq!(entry.affected_rows, None);
        assert_eq!(serde_json::to_value(entry).unwrap(), saved);
    }
}
