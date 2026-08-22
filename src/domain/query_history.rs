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
    pub fn new(
        query: String,
        executed_at: String,
        connection_id: ConnectionId,
        result_status: QueryResultStatus,
        affected_rows: Option<u64>,
    ) -> Self {
        Self::new_with_database(
            query,
            executed_at,
            connection_id,
            None,
            result_status,
            affected_rows,
        )
    }

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
    fn serde_round_trip() {
        let entry = QueryHistoryEntry::new(
            "SELECT * FROM users".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn serde_round_trip_with_affected_rows() {
        let entry = QueryHistoryEntry::new(
            "UPDATE users SET name = 'x'".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            QueryResultStatus::Success,
            Some(5),
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
        assert_eq!(deserialized.result_status, QueryResultStatus::Success);
        assert_eq!(deserialized.affected_rows, Some(5));
    }

    #[test]
    fn serde_json_format() {
        let entry = QueryHistoryEntry::new(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("abc-123"),
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains("\"query\":\"SELECT 1\""));
        assert!(json.contains("\"executed_at\":\"2026-03-13T12:00:00Z\""));
        assert!(json.contains("\"connection_id\":\"abc-123\""));
        assert!(json.contains("\"result_status\":\"Success\""));
    }

    #[test]
    fn serde_reads_entries_without_database() {
        let json = r#"{
            "query":"SELECT 1",
            "executed_at":"2026-03-13T12:00:00Z",
            "connection_id":"abc-123",
            "result_status":"Success",
            "affected_rows":null
        }"#;

        let entry: QueryHistoryEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.database, None);
    }

    #[test]
    fn serde_round_trip_preserves_database() {
        let entry = QueryHistoryEntry::new_with_database(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("abc-123"),
            Some("analytics".to_string()),
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.database.as_deref(), Some("analytics"));
    }
}
