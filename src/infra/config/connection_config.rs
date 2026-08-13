use serde::{Deserialize, Serialize};

use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionName, ConnectionProfile, ConnectionProfileError,
    DatabaseType, MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig,
    SqliteConnectionConfig, SslMode,
};

pub const CURRENT_VERSION: u32 = 3;
// Version 2 remains readable because older config files omit db_type and map to PostgreSQL.
const SUPPORTED_CONFIG_VERSIONS: &[u32] = &[2, CURRENT_VERSION];

pub fn is_supported_config_version(version: u32) -> bool {
    SUPPORTED_CONFIG_VERSIONS.contains(&version)
}

#[derive(Debug, Deserialize)]
pub struct ConfigVersionCheck {
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub er_browser: Option<String>,
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
    pub mysql_ssl_mode: Option<MySqlSslMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mysql_ssl_ca: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mysql_ssl_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mysql_ssl_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl From<&[ConnectionProfile]> for ConnectionConfigFile {
    fn from(profiles: &[ConnectionProfile]) -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: None,
            keymap_preset: None,
            er_browser: None,
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
            mysql_ssl_mode: None,
            mysql_ssl_ca: None,
            mysql_ssl_cert: None,
            mysql_ssl_key: None,
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
            ConnectionConfig::MySQL(config) => {
                entry.host = Some(config.host.clone());
                entry.port = Some(config.port);
                entry.database.clone_from(&config.database);
                entry.username = Some(config.username.clone());
                entry.password = Some(config.password.clone());
                entry.mysql_ssl_mode = Some(config.ssl_mode);
                entry.mysql_ssl_ca.clone_from(&config.ssl_ca);
                entry.mysql_ssl_cert.clone_from(&config.ssl_cert);
                entry.mysql_ssl_key.clone_from(&config.ssl_key);
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
            DatabaseType::PostgreSQL => Self::with_id_and_config(
                id,
                name.as_str().to_string(),
                ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    optional_postgres_field(entry.host.as_ref()),
                    entry.port.unwrap_or(5432),
                    required_postgres_field(entry.database.as_ref(), "database")?,
                    optional_postgres_field(entry.username.as_ref()),
                    match &entry.password {
                        Some(password) => password.clone(),
                        None => String::new(),
                    },
                    entry.ssl_mode.unwrap_or(SslMode::Prefer),
                )),
            ),
            DatabaseType::SQLite => Self::with_id_and_config(
                id,
                name.as_str().to_string(),
                ConnectionConfig::SQLite(SqliteConnectionConfig::new(required_sqlite_path(
                    entry.path.as_ref(),
                )?)?),
            ),
            DatabaseType::MySQL => {
                let database = entry.database.clone().filter(|value| !value.is_empty());
                Self::with_id_and_config(
                    id,
                    name.as_str().to_string(),
                    ConnectionConfig::MySQL(
                        MySqlConnectionConfig::new(
                            required_mysql_host(entry.host.as_ref())?,
                            entry.port.unwrap_or(3306),
                            database,
                            required_mysql_field(entry.username.as_ref(), "username")?,
                            entry.password.clone().unwrap_or_default(),
                            entry.mysql_ssl_mode.unwrap_or_default(),
                        )
                        .with_tls_paths(
                            entry.mysql_ssl_ca.clone(),
                            entry.mysql_ssl_cert.clone(),
                            entry.mysql_ssl_key.clone(),
                        ),
                    ),
                )
            }
        }
    }
}

fn required_sqlite_path(value: Option<&String>) -> Result<String, ConnectionProfileError> {
    value
        .cloned()
        .ok_or(ConnectionProfileError::EmptySqlitePath)
}

fn required_postgres_field(
    value: Option<&String>,
    field: &'static str,
) -> Result<String, ConnectionProfileError> {
    let value = value.ok_or(ConnectionProfileError::MissingPostgresField(field))?;
    if value.trim().is_empty() {
        return Err(ConnectionProfileError::MissingPostgresField(field));
    }
    Ok(value.clone())
}

fn optional_postgres_field(value: Option<&String>) -> String {
    value.cloned().unwrap_or_default()
}

fn required_mysql_field(
    value: Option<&String>,
    field: &'static str,
) -> Result<String, ConnectionProfileError> {
    let value = value.ok_or(ConnectionProfileError::MissingMySqlField(field))?;
    if value.trim().is_empty() {
        return Err(ConnectionProfileError::MissingMySqlField(field));
    }
    Ok(value.clone())
}

fn required_mysql_host(value: Option<&String>) -> Result<String, ConnectionProfileError> {
    let host = required_mysql_field(value, "host")?;
    if !MySqlConnectionConfig::is_valid_host(host.trim()) {
        return Err(ConnectionProfileError::InvalidMySqlHost);
    }
    Ok(host.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_versions_are_accepted() {
        for version in SUPPORTED_CONFIG_VERSIONS {
            assert!(is_supported_config_version(*version));
        }
    }

    #[test]
    fn unsupported_versions_are_rejected() {
        for version in [1, CURRENT_VERSION + 1] {
            assert!(!is_supported_config_version(version));
        }
    }

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
            mysql_ssl_mode: None,
            mysql_ssl_ca: None,
            mysql_ssl_cert: None,
            mysql_ssl_key: None,
            path: None,
        }
    }

    fn sqlite_entry(path: Option<&str>) -> ConnectionConfigEntry {
        ConnectionConfigEntry {
            id: "sqlite-id".to_string(),
            name: "Local".to_string(),
            db_type: DatabaseType::SQLite,
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            ssl_mode: None,
            mysql_ssl_mode: None,
            mysql_ssl_ca: None,
            mysql_ssl_cert: None,
            mysql_ssl_key: None,
            path: path.map(str::to_string),
        }
    }

    fn mysql_entry(database: Option<&str>) -> ConnectionConfigEntry {
        ConnectionConfigEntry {
            id: "mysql-id".to_string(),
            name: "MySQL".to_string(),
            db_type: DatabaseType::MySQL,
            host: Some("localhost".to_string()),
            port: Some(3306),
            database: database.map(str::to_string),
            username: Some("user".to_string()),
            password: Some("p@ss#word".to_string()),
            ssl_mode: None,
            mysql_ssl_mode: Some(MySqlSslMode::Required),
            mysql_ssl_ca: None,
            mysql_ssl_cert: None,
            mysql_ssl_key: None,
            path: None,
        }
    }

    #[test]
    fn postgres_entry_rejects_missing_required_field() {
        let mut entry = postgres_entry();
        entry.database = None;

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::MissingPostgresField("database"))
        ));
    }

    #[test]
    fn v2_entry_defaults_to_postgres() {
        let entry: ConnectionConfigEntry = serde_json::from_str(
            r#"{
                "id": "legacy-id",
                "name": "Legacy",
                "host": "localhost",
                "port": 5432,
                "database": "app",
                "username": "user",
                "password": "secret",
                "ssl_mode": "prefer"
            }"#,
        )
        .unwrap();

        let profile = ConnectionProfile::try_from(&entry).unwrap();

        assert_eq!(profile.database_type(), DatabaseType::PostgreSQL);
    }

    #[test]
    fn sqlite_entry_rejects_missing_path() {
        let entry = sqlite_entry(None);

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::EmptySqlitePath)
        ));
    }

    #[test]
    fn sqlite_entry_rejects_empty_path() {
        let entry = sqlite_entry(Some(""));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::EmptySqlitePath)
        ));
    }

    #[test]
    fn sqlite_entry_rejects_invalid_path() {
        let entry = sqlite_entry(Some("/tmp/app\0.db"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::InvalidSqlitePath)
        ));
    }

    #[test]
    fn sqlite_entry_rejects_in_memory_database() {
        let entry = sqlite_entry(Some(":memory:"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::UnsupportedSqliteInMemoryDatabase)
        ));
    }

    #[test]
    fn sqlite_entry_rejects_uri_filename() {
        let entry = sqlite_entry(Some("file:/tmp/app.db?mode=ro"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::UnsupportedSqliteUriFilename)
        ));
    }

    #[test]
    fn mysql_entry_round_trips_optional_database_and_tls_mode() {
        let entry = mysql_entry(Some("app"));
        let profile = ConnectionProfile::try_from(&entry).unwrap();
        let serialized = ConnectionConfigEntry::from(&profile);

        assert_eq!(serialized.db_type, DatabaseType::MySQL);
        assert_eq!(serialized.database.as_deref(), Some("app"));
        assert_eq!(serialized.mysql_ssl_mode, Some(MySqlSslMode::Required));
        assert_eq!(serialized.port, Some(3306));
        assert_eq!(serialized.password.as_deref(), Some("p@ss#word"));
    }

    #[test]
    fn mysql_entry_round_trips_certificate_paths() {
        let mut entry = mysql_entry(Some("app"));
        entry.mysql_ssl_mode = Some(MySqlSslMode::VerifyIdentity);
        entry.mysql_ssl_ca = Some("/tmp/ca #1.pem".to_string());
        entry.mysql_ssl_cert = Some(r"C:\certs\client.pem".to_string());
        entry.mysql_ssl_key = Some(r"C:\certs\client-key.pem".to_string());

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        let config = profile.mysql_config().unwrap();
        assert_eq!(config.ssl_mode, MySqlSslMode::VerifyIdentity);
        assert_eq!(config.ssl_ca.as_deref(), Some("/tmp/ca #1.pem"));
        assert_eq!(config.ssl_cert.as_deref(), Some(r"C:\certs\client.pem"));
        assert_eq!(config.ssl_key.as_deref(), Some(r"C:\certs\client-key.pem"));

        let serialized = ConnectionConfigEntry::from(&profile);
        assert_eq!(serialized.mysql_ssl_ca, entry.mysql_ssl_ca);
        assert_eq!(serialized.mysql_ssl_cert, entry.mysql_ssl_cert);
        assert_eq!(serialized.mysql_ssl_key, entry.mysql_ssl_key);
    }

    #[test]
    fn mysql_entry_without_database_is_valid() {
        let profile = ConnectionProfile::try_from(&mysql_entry(None)).unwrap();

        let config = profile.mysql_config().unwrap();
        assert_eq!(config.database, None);
        assert_eq!(config.ssl_mode, MySqlSslMode::Required);
    }

    #[test]
    fn mysql_entry_requires_host_and_username() {
        let mut entry = mysql_entry(None);
        entry.host = None;
        assert!(matches!(
            ConnectionProfile::try_from(&entry),
            Err(ConnectionProfileError::MissingMySqlField("host"))
        ));

        entry.host = Some("localhost".to_string());
        entry.username = Some(" ".to_string());
        assert!(matches!(
            ConnectionProfile::try_from(&entry),
            Err(ConnectionProfileError::MissingMySqlField("username"))
        ));
    }

    #[test]
    fn mysql_entry_rejects_invalid_host() {
        let mut entry = mysql_entry(None);
        entry.host = Some("db example".to_string());

        assert!(matches!(
            ConnectionProfile::try_from(&entry),
            Err(ConnectionProfileError::InvalidMySqlHost)
        ));
    }
}
