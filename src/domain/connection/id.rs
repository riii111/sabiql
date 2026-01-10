use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a connection profile
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(String);

impl ConnectionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_generates_unique_ids() {
        let id1 = ConnectionId::new();
        let id2 = ConnectionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn from_string_preserves_value() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = ConnectionId::from_string(uuid_str);
        assert_eq!(id.as_str(), uuid_str);
    }

    #[test]
    fn display_shows_uuid() {
        let uuid_str = "test-uuid";
        let id = ConnectionId::from_string(uuid_str);
        assert_eq!(format!("{}", id), uuid_str);
    }
}
