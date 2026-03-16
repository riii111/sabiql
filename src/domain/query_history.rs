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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryHistoryEntry {
    pub query: String,
    pub executed_at: Iso8601Timestamp,
    pub connection_id: ConnectionId,
}

impl QueryHistoryEntry {
    pub fn new(query: String, executed_at: String, connection_id: ConnectionId) -> Self {
        Self {
            query,
            executed_at: Iso8601Timestamp::new(executed_at),
            connection_id,
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
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn serde_json_format() {
        let entry = QueryHistoryEntry::new(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("abc-123"),
        );

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains("\"query\":\"SELECT 1\""));
        assert!(json.contains("\"executed_at\":\"2026-03-13T12:00:00Z\""));
        assert!(json.contains("\"connection_id\":\"abc-123\""));
    }
}
