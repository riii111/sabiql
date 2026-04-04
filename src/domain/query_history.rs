use serde::{Deserialize, Serialize};

use super::connection::ConnectionId;

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
        Self {
            query,
            executed_at: Iso8601Timestamp::new(executed_at),
            connection_id,
            result_status,
            affected_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_history_round_trip_preserves_entry() {
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
    fn query_history_round_trip_preserves_affected_rows() {
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
    fn query_history_round_trip_serializes_expected_json() {
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
}
