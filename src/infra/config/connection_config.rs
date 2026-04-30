use serde::{Deserialize, Serialize};

use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionName, ConnectionProfile, ConnectionProfileError,
    DatabaseType, PostgresConnectionConfig, SqliteConnectionConfig, SslMode,
};

pub const CURRENT_VERSION: u32 = 3;

#[derive(Debug, Deserialize)]
pub struct ConfigVersionCheck {
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigFile {
    pub version: u32,
    pub connections: Vec<ConnectionConfigEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub db_type: DatabaseType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<SslMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl From<&[ConnectionProfile]> for ConnectionConfigFile {
    fn from(profiles: &[ConnectionProfile]) -> Self {
        Self {
            version: CURRENT_VERSION,
            connections: profiles.iter().map(ConnectionConfigEntry::from).collect(),
        }
    }
}

impl TryFrom<&ConnectionConfigFile> for Vec<ConnectionProfile> {
    type Error = ConnectionProfileError;

    fn try_from(config: &ConnectionConfigFile) -> Result<Self, Self::Error> {
        config
            .connections
            .iter()
            .map(ConnectionProfile::try_from)
            .collect()
    }
}

impl From<&ConnectionProfile> for ConnectionConfigEntry {
    fn from(profile: &ConnectionProfile) -> Self {
        let mut entry = Self {
            id: profile.id.as_str().to_string(),
            name: profile.name.as_str().to_string(),
            db_type: profile.database_type(),
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            ssl_mode: None,
            path: None,
        };
        match &profile.config {
            ConnectionConfig::PostgreSQL(config) => {
                entry.host = Some(config.host.clone());
                entry.port = Some(config.port);
                entry.database = Some(config.database.clone());
                entry.username = Some(config.username.clone());
                entry.password = Some(config.password.clone());
                entry.ssl_mode = Some(config.ssl_mode);
            }
            ConnectionConfig::SQLite(config) => {
                entry.path = Some(config.path().to_string());
            }
        }
        entry
    }
}

impl TryFrom<&ConnectionConfigEntry> for ConnectionProfile {
    type Error = ConnectionProfileError;

    fn try_from(entry: &ConnectionConfigEntry) -> Result<Self, Self::Error> {
        let id = ConnectionId::from_string(&entry.id);
        let name = ConnectionName::new(&entry.name)?;
        match entry.db_type {
            DatabaseType::PostgreSQL => ConnectionProfile::with_id_and_config(
                id,
                name.as_str().to_string(),
                ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    required_postgres_field(&entry.host, "host")?,
                    entry.port.unwrap_or(5432),
                    required_postgres_field(&entry.database, "database")?,
                    required_postgres_field(&entry.username, "username")?,
                    entry.password.clone().unwrap_or_default(),
                    entry.ssl_mode.unwrap_or_default(),
                )),
            ),
            DatabaseType::SQLite => ConnectionProfile::with_id_and_config(
                id,
                name.as_str().to_string(),
                ConnectionConfig::SQLite(SqliteConnectionConfig::new(
                    entry.path.clone().unwrap_or_default(),
                )?),
            ),
        }
    }
}

fn required_postgres_field(
    value: &Option<String>,
    field: &'static str,
) -> Result<String, ConnectionProfileError> {
    let value = value
        .as_ref()
        .ok_or(ConnectionProfileError::MissingPostgresField(field))?;
    if value.trim().is_empty() {
        return Err(ConnectionProfileError::MissingPostgresField(field));
    }
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn postgres_entry() -> ConnectionConfigEntry {
        ConnectionConfigEntry {
            id: "test-id".to_string(),
            name: "Test".to_string(),
            db_type: DatabaseType::PostgreSQL,
            host: Some("localhost".to_string()),
            port: Some(5432),
            database: Some("app".to_string()),
            username: Some("user".to_string()),
            password: None,
            ssl_mode: Some(SslMode::Prefer),
            path: None,
        }
    }

    #[test]
    fn postgres_entry_rejects_missing_required_field() {
        let mut entry = postgres_entry();
        entry.host = None;

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::MissingPostgresField("host"))
        ));
    }

    #[test]
    fn sqlite_entry_rejects_invalid_path() {
        let entry = ConnectionConfigEntry {
            id: "sqlite-id".to_string(),
            name: "Local".to_string(),
            db_type: DatabaseType::SQLite,
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            ssl_mode: None,
            path: Some("/tmp/app\0.db".to_string()),
        };

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::InvalidSqlitePath)
        ));
    }
}
