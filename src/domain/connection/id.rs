use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(String);

const EPHEMERAL_CLI_ID: &str = "__cli_ephemeral__";

impl ConnectionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn ephemeral_cli() -> Self {
        Self::from_string(EPHEMERAL_CLI_ID)
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn is_ephemeral_cli(&self) -> bool {
        self.0 == EPHEMERAL_CLI_ID
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ConnectionId {
    fn as_ref(&self) -> &str {
        self.as_str()
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
        assert_eq!(format!("{id}"), uuid_str);
    }

    #[test]
    fn ephemeral_cli_id_is_stable() {
        let id = ConnectionId::ephemeral_cli();
        assert!(id.is_ephemeral_cli());
        assert_eq!(id.as_str(), "__cli_ephemeral__");
    }

    #[test]
    fn regular_id_is_not_ephemeral_cli() {
        let id = ConnectionId::new();
        assert!(!id.is_ephemeral_cli());
    }
}
