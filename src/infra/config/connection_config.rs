use serde::{Deserialize, Serialize};

use crate::domain::connection::{
    ConnectionId, ConnectionName, ConnectionProfile, ConnectionProfileError, DatabaseType,
    PostgresConnectionConfig, SqliteConnectionConfig, SslMode,
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
            crate::domain::ConnectionConfig::PostgreSQL(config) => {
                entry.host = Some(config.host.clone());
                entry.port = Some(config.port);
                entry.database = Some(config.database.clone());
                entry.username = Some(config.username.clone());
                entry.password = Some(config.password.clone());
                entry.ssl_mode = Some(config.ssl_mode);
            }
            crate::domain::ConnectionConfig::SQLite(config) => {
                entry.path = Some(config.path.clone());
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
                crate::domain::ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    entry.host.clone().unwrap_or_default(),
                    entry.port.unwrap_or(5432),
                    entry.database.clone().unwrap_or_default(),
                    entry.username.clone().unwrap_or_default(),
                    entry.password.clone().unwrap_or_default(),
                    entry.ssl_mode.unwrap_or_default(),
                )),
            ),
            DatabaseType::SQLite => ConnectionProfile::with_id_and_config(
                id,
                name.as_str().to_string(),
                crate::domain::ConnectionConfig::SQLite(SqliteConnectionConfig::new(
                    entry.path.clone().unwrap_or_default(),
                )),
            ),
        }
    }
}
