use serde::{Deserialize, Serialize};
use uuid::{Uuid, uuid};

use super::connection::{ConnectionConfig, ConnectionId, MySqlTransport};

const QUERY_HISTORY_KEY_NAMESPACE: Uuid = uuid!("c40ec5b6-75b4-4e6f-a733-b3aa58c327b2");

/// Identifies which database target a history entry belongs to.
///
/// Saved profiles derive it from their connection properties rather than the
/// profile's random id, so deleting a profile and re-adding one for the same
/// target keeps its history. The value is always safe to use as a file name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueryHistoryKey(String);

impl QueryHistoryKey {
    /// Passwords and TLS options are deliberately excluded: they change how
    /// sabiql connects, not which database it talks to.
    pub fn for_config(config: &ConnectionConfig) -> Self {
        match config {
            ConnectionConfig::PostgreSQL(pg) => Self::derive(
                "postgres",
                &[
                    &pg.host.to_ascii_lowercase(),
                    &pg.port.to_string(),
                    &pg.database,
                    &pg.username,
                ],
            ),
            // The database is not part of a MySQL key because one connection
            // can switch databases; entries record their database instead.
            ConnectionConfig::MySQL(mysql) => match mysql.transport {
                MySqlTransport::Tcp => Self::derive(
                    "mysql",
                    &[
                        "tcp",
                        &mysql.host.to_ascii_lowercase(),
                        &mysql.port.to_string(),
                        &mysql.username,
                    ],
                ),
                MySqlTransport::UnixSocket | MySqlTransport::NamedPipe => Self::derive(
                    "mysql",
                    &[
                        "local",
                        mysql.transport_path.as_deref().unwrap_or_default(),
                        &mysql.username,
                    ],
                ),
            },
            ConnectionConfig::SQLite(sqlite) => Self::derive("sqlite", &[sqlite.path()]),
        }
    }

    /// For targets without a saved profile (pg_service entries, CLI SQLite),
    /// whose connection ids are already derived from the target.
    pub fn for_connection_id(id: &ConnectionId) -> Self {
        Self::derive("id", &[id.as_str()])
    }

    fn derive(engine: &str, parts: &[&str]) -> Self {
        let mut input = String::from(engine);
        for part in parts {
            input.push('\0');
            input.push_str(part);
        }
        let hash = Uuid::new_v5(&QUERY_HISTORY_KEY_NAMESPACE, input.as_bytes());
        Self(format!("{engine}-{}", hash.as_simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for QueryHistoryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHistoryScope {
    pub connection_id: ConnectionId,
    pub key: QueryHistoryKey,
    pub database: Option<String>,
}

impl QueryHistoryScope {
    pub fn new(
        connection_id: ConnectionId,
        key: QueryHistoryKey,
        database: Option<String>,
    ) -> Self {
        Self {
            connection_id,
            key,
            database,
        }
    }

    /// Entries written before history keys existed carry only the profile id.
    pub fn matches(&self, entry: &QueryHistoryEntry) -> bool {
        let same_target = entry.connection_key.as_ref().map_or_else(
            || entry.connection_id == self.connection_id,
            |key| key == &self.key,
        );
        same_target && entry.database == self.database
    }

    pub fn entry(
        &self,
        query: String,
        executed_at: String,
        result_status: QueryResultStatus,
        affected_rows: Option<u64>,
    ) -> QueryHistoryEntry {
        QueryHistoryEntry {
            connection_key: Some(self.key.clone()),
            ..QueryHistoryEntry::new_with_database(
                query,
                executed_at,
                self.connection_id.clone(),
                self.database.clone(),
                result_status,
                affected_rows,
            )
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
    pub connection_key: Option<QueryHistoryKey>,
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
            connection_key: None,
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
        let entry = QueryHistoryEntry::new_with_database(
            "SELECT * FROM users".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            None,
            QueryResultStatus::Success,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn serde_round_trip_with_affected_rows() {
        let entry = QueryHistoryEntry::new_with_database(
            "UPDATE users SET name = 'x'".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-uuid"),
            None,
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
        let entry = QueryHistoryEntry::new_with_database(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("abc-123"),
            None,
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

    mod history_key {
        use super::*;
        use crate::connection::{ConnectionProfile, SslMode};

        fn postgres(host: &str, database: &str, password: &str) -> ConnectionConfig {
            ConnectionProfile::new_postgres(
                "pg",
                host,
                5432,
                database,
                "app",
                password,
                SslMode::Prefer,
            )
            .unwrap()
            .config
        }

        #[test]
        fn same_target_gives_same_key_across_profiles() {
            let first = ConnectionProfile::new_postgres(
                "a",
                "db.local",
                5432,
                "app",
                "app",
                "x",
                SslMode::Prefer,
            )
            .unwrap();
            let second = ConnectionProfile::new_postgres(
                "b",
                "db.local",
                5432,
                "app",
                "app",
                "x",
                SslMode::Prefer,
            )
            .unwrap();

            assert_ne!(first.id, second.id);
            assert_eq!(
                QueryHistoryKey::for_config(&first.config),
                QueryHistoryKey::for_config(&second.config)
            );
        }

        #[test]
        fn password_and_host_case_do_not_change_key() {
            assert_eq!(
                QueryHistoryKey::for_config(&postgres("DB.local", "app", "old")),
                QueryHistoryKey::for_config(&postgres("db.local", "app", "new"))
            );
        }

        #[test]
        fn different_database_gives_different_key() {
            assert_ne!(
                QueryHistoryKey::for_config(&postgres("db.local", "app", "x")),
                QueryHistoryKey::for_config(&postgres("db.local", "analytics", "x"))
            );
        }

        #[test]
        fn key_is_stable_and_file_name_safe() {
            let key =
                QueryHistoryKey::for_connection_id(&ConnectionId::from_string("service:my/db"));

            assert_eq!(
                key,
                QueryHistoryKey::for_connection_id(&ConnectionId::from_string("service:my/db"))
            );
            assert!(
                key.as_str()
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            );
        }
    }

    mod scope_matches {
        use super::*;

        fn scope() -> QueryHistoryScope {
            QueryHistoryScope::new(
                ConnectionId::from_string("profile-1"),
                QueryHistoryKey::for_connection_id(&ConnectionId::from_string("target")),
                None,
            )
        }

        fn legacy_entry(connection_id: &str) -> QueryHistoryEntry {
            QueryHistoryEntry::new_with_database(
                "SELECT 1".to_string(),
                "2026-03-13T12:00:00Z".to_string(),
                ConnectionId::from_string(connection_id),
                None,
                QueryResultStatus::Success,
                None,
            )
        }

        #[test]
        fn matches_keyed_entry_from_another_profile_id() {
            let other_profile = QueryHistoryScope {
                connection_id: ConnectionId::from_string("deleted-profile"),
                ..scope()
            };
            let entry = other_profile.entry(
                "SELECT 1".into(),
                "2026-03-13T12:00:00Z".into(),
                QueryResultStatus::Success,
                None,
            );

            assert!(scope().matches(&entry));
        }

        #[test]
        fn matches_legacy_entry_by_profile_id() {
            assert!(scope().matches(&legacy_entry("profile-1")));
            assert!(!scope().matches(&legacy_entry("profile-2")));
        }

        #[test]
        fn rejects_entry_for_another_database() {
            let mysql_scope = QueryHistoryScope {
                database: Some("app".to_string()),
                ..scope()
            };

            assert!(!mysql_scope.matches(&legacy_entry("profile-1")));
        }

        #[test]
        fn serde_round_trip_preserves_connection_key() {
            let entry = scope().entry(
                "SELECT 1".into(),
                "2026-03-13T12:00:00Z".into(),
                QueryResultStatus::Success,
                None,
            );

            let json = serde_json::to_string(&entry).unwrap();
            let deserialized: QueryHistoryEntry = serde_json::from_str(&json).unwrap();

            assert_eq!(deserialized.connection_key, Some(scope().key));
        }
    }
}
