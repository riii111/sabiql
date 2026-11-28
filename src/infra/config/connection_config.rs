#![allow(
    clippy::redundant_pub_crate,
    reason = "config types and version helpers are crate-visible through the config facade but the implementation module stays internal"
)]

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionName, ConnectionProfile, ConnectionProfileError,
    DatabaseType, MySqlConnectionConfig, MySqlSslMode, MySqlTransport, PostgresConnectionConfig,
    SqliteConnectionConfig, SqliteConnectionConfigError, SslMode,
};

pub(crate) const CURRENT_VERSION: u32 = 3;
// Version 2 remains readable because older config files omit db_type and map to PostgreSQL.
const SUPPORTED_CONFIG_VERSIONS: &[u32] = &[2, CURRENT_VERSION];

pub(crate) fn is_supported_config_version(version: u32) -> bool {
    SUPPORTED_CONFIG_VERSIONS.contains(&version)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConfigVersionCheck {
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ConnectionConfigFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub er_browser: Option<String>,
    pub connections: Vec<ConnectionConfigEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ConnectionConfigEntry {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub engine: EngineConfigEntry,
}

#[derive(Debug, Serialize)]
#[serde(tag = "db_type")]
pub(crate) enum EngineConfigEntry {
    #[serde(rename = "postgresql")]
    PostgreSQL(PostgresConfigEntry),
    #[serde(rename = "sqlite")]
    SQLite(SqliteConfigEntry),
    #[serde(rename = "mysql")]
    MySQL(MySqlConfigEntry),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PostgresConfigEntry {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_postgres_port")]
    pub port: u16,
    pub database: Option<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub ssl_mode: SslMode,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SqliteConfigEntry {
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MySqlConfigEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub username: Option<String>,
    #[serde(default)]
    pub password: String,
    #[serde(rename = "mysql_ssl_mode", default)]
    pub ssl_mode: MySqlSslMode,
    #[serde(rename = "mysql_ssl_ca", skip_serializing_if = "Option::is_none")]
    pub ssl_ca: Option<String>,
    #[serde(rename = "mysql_ssl_cert", skip_serializing_if = "Option::is_none")]
    pub ssl_cert: Option<String>,
    #[serde(rename = "mysql_ssl_key", skip_serializing_if = "Option::is_none")]
    pub ssl_key: Option<String>,
    #[serde(
        rename = "mysql_server_public_key_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub server_public_key_path: Option<String>,
    #[serde(
        rename = "mysql_enable_cleartext_plugin",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub enable_cleartext_plugin: bool,
    #[serde(rename = "mysql_transport", default, skip_serializing_if = "is_tcp")]
    pub transport: MySqlTransport,
    #[serde(
        rename = "mysql_transport_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub transport_path: Option<String>,
}

const fn default_postgres_port() -> u16 {
    5432
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes the field by reference"
)]
fn is_tcp(transport: &MySqlTransport) -> bool {
    *transport == MySqlTransport::Tcp
}

impl<'de> Deserialize<'de> for EngineConfigEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Version 2 entries omit db_type, so a missing tag selects PostgreSQL instead of failing like serde's internal tagging would.
        #[derive(Deserialize)]
        struct Tagged {
            #[serde(default)]
            db_type: DatabaseType,
            #[serde(flatten)]
            fields: toml::Table,
        }

        let Tagged { db_type, fields } = Tagged::deserialize(deserializer)?;
        let fields = toml::Value::Table(fields);
        match db_type {
            DatabaseType::PostgreSQL => fields.try_into().map(Self::PostgreSQL),
            DatabaseType::SQLite => fields.try_into().map(Self::SQLite),
            DatabaseType::MySQL => fields.try_into().map(Self::MySQL),
        }
        .map_err(D::Error::custom)
    }
}

impl Default for ConnectionConfigFile {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: None,
            icon_mode: None,
            keymap_preset: None,
            er_browser: None,
            connections: Vec::new(),
        }
    }
}

impl ConnectionConfigFile {
    pub(crate) fn profiles(&self) -> Result<Vec<ConnectionProfile>, ConnectionProfileError> {
        self.connections
            .iter()
            .map(ConnectionProfile::try_from)
            .collect()
    }

    pub(crate) fn set_profiles(&mut self, profiles: &[ConnectionProfile]) {
        self.version = CURRENT_VERSION;
        self.connections = profiles.iter().map(ConnectionConfigEntry::from).collect();
    }
}

impl From<&ConnectionProfile> for ConnectionConfigEntry {
    fn from(profile: &ConnectionProfile) -> Self {
        let engine = match &profile.config {
            ConnectionConfig::PostgreSQL(config) => {
                EngineConfigEntry::PostgreSQL(PostgresConfigEntry {
                    host: config.host.clone(),
                    port: config.port,
                    database: Some(config.database.clone()),
                    username: config.username.clone(),
                    password: config.password.clone(),
                    ssl_mode: config.ssl_mode,
                })
            }
            ConnectionConfig::SQLite(config) => EngineConfigEntry::SQLite(SqliteConfigEntry {
                path: Some(config.path().to_string()),
            }),
            ConnectionConfig::MySQL(config) => {
                let tcp = config.transport == MySqlTransport::Tcp;
                EngineConfigEntry::MySQL(MySqlConfigEntry {
                    host: tcp.then(|| config.host.clone()),
                    port: tcp.then_some(config.port),
                    database: config.database.clone(),
                    username: Some(config.username.clone()),
                    password: config.password.clone(),
                    ssl_mode: config.ssl_mode,
                    ssl_ca: config
                        .ssl_mode
                        .uses_ca()
                        .then(|| config.ssl_ca.clone())
                        .flatten(),
                    ssl_cert: config.ssl_cert.clone(),
                    ssl_key: config.ssl_key.clone(),
                    server_public_key_path: config.server_public_key_path.clone(),
                    enable_cleartext_plugin: config.enable_cleartext_plugin,
                    transport: config.transport,
                    transport_path: config.transport_path.clone(),
                })
            }
        };
        Self {
            id: profile.id.as_str().to_string(),
            name: profile.name.as_str().to_string(),
            engine,
        }
    }
}

impl TryFrom<&ConnectionConfigEntry> for ConnectionProfile {
    type Error = ConnectionProfileError;

    fn try_from(entry: &ConnectionConfigEntry) -> Result<Self, Self::Error> {
        let id = ConnectionId::from_string(&entry.id);
        let name = ConnectionName::new(&entry.name)?.as_str().to_string();
        let config = match &entry.engine {
            EngineConfigEntry::PostgreSQL(entry) => {
                ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    entry.host.clone(),
                    entry.port,
                    required_postgres_field(entry.database.as_ref(), "database")?,
                    entry.username.clone(),
                    entry.password.clone(),
                    entry.ssl_mode,
                ))
            }
            EngineConfigEntry::SQLite(entry) => ConnectionConfig::SQLite(
                SqliteConnectionConfig::new(required_sqlite_path(entry.path.as_ref())?)?,
            ),
            EngineConfigEntry::MySQL(entry) => ConnectionConfig::MySQL(
                MySqlConnectionConfig::new(
                    match entry.transport {
                        MySqlTransport::Tcp => required_mysql_host(entry.host.as_ref())?,
                        MySqlTransport::UnixSocket | MySqlTransport::NamedPipe => {
                            entry.host.clone().unwrap_or_default()
                        }
                    },
                    entry.port.unwrap_or(3306),
                    entry.database.clone().filter(|value| !value.is_empty()),
                    required_mysql_field(entry.username.as_ref(), "username")?,
                    entry.password.clone(),
                    entry.ssl_mode,
                )
                .with_tls_paths(
                    entry.ssl_ca.clone(),
                    entry.ssl_cert.clone(),
                    entry.ssl_key.clone(),
                )
                .with_server_public_key_path(entry.server_public_key_path.clone())
                .with_transport(entry.transport, entry.transport_path.clone())
                .with_cleartext_auth_plugin(entry.enable_cleartext_plugin),
            ),
        };
        Self::with_id_and_config(id, name, config)
    }
}

fn required_sqlite_path(value: Option<&String>) -> Result<String, ConnectionProfileError> {
    value
        .cloned()
        .ok_or_else(|| SqliteConnectionConfigError::EmptyPath.into())
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
    use crate::domain::connection::{ConnectionEndpoint, build_dsn};

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
            engine: EngineConfigEntry::PostgreSQL(PostgresConfigEntry {
                host: "localhost".to_string(),
                port: 5432,
                database: Some("app".to_string()),
                username: "user".to_string(),
                password: String::new(),
                ssl_mode: SslMode::Prefer,
            }),
        }
    }

    fn sqlite_entry(path: Option<&str>) -> ConnectionConfigEntry {
        ConnectionConfigEntry {
            id: "sqlite-id".to_string(),
            name: "Local".to_string(),
            engine: EngineConfigEntry::SQLite(SqliteConfigEntry {
                path: path.map(str::to_string),
            }),
        }
    }

    fn mysql_fields(database: Option<&str>) -> MySqlConfigEntry {
        MySqlConfigEntry {
            host: Some("localhost".to_string()),
            port: Some(3306),
            database: database.map(str::to_string),
            username: Some("user".to_string()),
            password: "p@ss#word".to_string(),
            ssl_mode: MySqlSslMode::Required,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
            server_public_key_path: None,
            enable_cleartext_plugin: false,
            transport: MySqlTransport::Tcp,
            transport_path: None,
        }
    }

    fn mysql_entry(fields: MySqlConfigEntry) -> ConnectionConfigEntry {
        ConnectionConfigEntry {
            id: "mysql-id".to_string(),
            name: "MySQL".to_string(),
            engine: EngineConfigEntry::MySQL(fields),
        }
    }

    fn serialized_mysql_fields(profile: &ConnectionProfile) -> MySqlConfigEntry {
        match ConnectionConfigEntry::from(profile).engine {
            EngineConfigEntry::MySQL(fields) => fields,
            other => panic!("expected MySQL entry, got {other:?}"),
        }
    }

    #[test]
    fn postgres_entry_rejects_missing_required_field() {
        let mut entry = postgres_entry();
        if let EngineConfigEntry::PostgreSQL(fields) = &mut entry.engine {
            fields.database = None;
        }

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
    fn entry_ignores_fields_of_other_engines() {
        let entry: ConnectionConfigEntry = toml::from_str(
            r#"
id = "legacy-id"
name = "Legacy"
database = "app"
path = "/tmp/ignored.db"
mysql_ssl_mode = "REQUIRED"
"#,
        )
        .unwrap();

        let profile = ConnectionProfile::try_from(&entry).unwrap();

        assert_eq!(
            profile.config,
            ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "",
                5432,
                "app",
                "",
                "",
                SslMode::Prefer
            ))
        );
    }

    #[test]
    fn entries_serialize_in_the_flat_file_layout() {
        let profiles = [
            ConnectionProfile::with_id_and_config(
                ConnectionId::from_string("pg"),
                "PG".to_string(),
                ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    "h",
                    6543,
                    "db",
                    "u",
                    "p",
                    SslMode::Require,
                )),
            )
            .unwrap(),
            ConnectionProfile::with_id_and_config(
                ConnectionId::from_string("lite"),
                "Lite".to_string(),
                ConnectionConfig::SQLite(SqliteConnectionConfig::new("/tmp/a.db").unwrap()),
            )
            .unwrap(),
            ConnectionProfile::try_from(&mysql_entry(MySqlConfigEntry {
                database: None,
                ssl_mode: MySqlSslMode::VerifyCa,
                ssl_ca: Some("/ca".to_string()),
                enable_cleartext_plugin: true,
                ..mysql_fields(None)
            }))
            .unwrap(),
        ];
        let mut file = ConnectionConfigFile::default();
        file.set_profiles(&profiles);

        assert_eq!(
            toml::to_string_pretty(&file).unwrap(),
            r#"version = 3

[[connections]]
id = "pg"
name = "PG"
db_type = "postgresql"
host = "h"
port = 6543
database = "db"
username = "u"
password = "p"
ssl_mode = "require"

[[connections]]
id = "lite"
name = "Lite"
db_type = "sqlite"
path = "/tmp/a.db"

[[connections]]
id = "mysql-id"
name = "MySQL"
db_type = "mysql"
host = "localhost"
port = 3306
username = "user"
password = "p@ss#word"
mysql_ssl_mode = "VERIFY_CA"
mysql_ssl_ca = "/ca"
mysql_enable_cleartext_plugin = true
"#
        );
    }

    #[test]
    fn sqlite_entry_rejects_missing_path() {
        let entry = sqlite_entry(None);

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::SqliteConfig(
                SqliteConnectionConfigError::EmptyPath
            ))
        ));
    }

    #[test]
    fn sqlite_entry_rejects_empty_path() {
        let entry = sqlite_entry(Some(""));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::SqliteConfig(
                SqliteConnectionConfigError::EmptyPath
            ))
        ));
    }

    #[test]
    fn sqlite_entry_rejects_invalid_path() {
        let entry = sqlite_entry(Some("/tmp/app\0.db"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::SqliteConfig(
                SqliteConnectionConfigError::UnsupportedPath
            ))
        ));
    }

    #[test]
    fn sqlite_entry_rejects_in_memory_database() {
        let entry = sqlite_entry(Some(":memory:"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::SqliteConfig(
                SqliteConnectionConfigError::UnsupportedInMemoryDatabase
            ))
        ));
    }

    #[test]
    fn sqlite_entry_rejects_uri_filename() {
        let entry = sqlite_entry(Some("file:/tmp/app.db?mode=ro"));

        let result = ConnectionProfile::try_from(&entry);

        assert!(matches!(
            result,
            Err(ConnectionProfileError::SqliteConfig(
                SqliteConnectionConfigError::UnsupportedUriFilename
            ))
        ));
    }

    #[test]
    fn mysql_entry_round_trips_optional_database_and_tls_mode() {
        let profile = ConnectionProfile::try_from(&mysql_entry(mysql_fields(Some("app")))).unwrap();
        let serialized = serialized_mysql_fields(&profile);

        assert_eq!(serialized.database.as_deref(), Some("app"));
        assert_eq!(serialized.ssl_mode, MySqlSslMode::Required);
        assert_eq!(serialized.port, Some(3306));
        assert_eq!(serialized.password, "p@ss#word");
    }

    #[cfg(unix)]
    #[test]
    fn mysql_socket_entry_round_trips_transport_path_without_tcp_fields() {
        let entry = mysql_entry(MySqlConfigEntry {
            transport: MySqlTransport::UnixSocket,
            transport_path: Some("/run/mysqld/mysqld.sock".to_string()),
            ..mysql_fields(Some("app"))
        });

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        let config = profile.config.as_mysql().unwrap();
        assert_eq!(config.transport, MySqlTransport::UnixSocket);
        assert_eq!(
            config.transport_path.as_deref(),
            Some("/run/mysqld/mysqld.sock")
        );

        let serialized = ConnectionConfigEntry::from(&profile);
        let EngineConfigEntry::MySQL(fields) = &serialized.engine else {
            panic!("expected MySQL entry");
        };
        assert_eq!(fields.host, None);
        assert_eq!(fields.port, None);
        assert_eq!(fields.transport, MySqlTransport::UnixSocket);
        assert_eq!(
            fields.transport_path.as_deref(),
            Some("/run/mysqld/mysqld.sock")
        );

        let round_tripped = ConnectionProfile::try_from(&serialized).unwrap();
        let dsn = build_dsn(&ConnectionEndpoint::from(&round_tripped.config));
        assert!(dsn.contains("transport=UNIX_SOCKET"));
        assert!(dsn.contains("transport-path=%2Frun%2Fmysqld%2Fmysqld.sock"));
    }

    #[test]
    fn mysql_entry_round_trips_cleartext_auth_plugin() {
        let entry = mysql_entry(MySqlConfigEntry {
            enable_cleartext_plugin: true,
            ..mysql_fields(Some("app"))
        });

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        assert!(profile.config.as_mysql().unwrap().enable_cleartext_plugin);

        assert!(serialized_mysql_fields(&profile).enable_cleartext_plugin);
    }

    #[test]
    fn mysql_entry_round_trips_certificate_paths() {
        let entry = mysql_entry(MySqlConfigEntry {
            ssl_mode: MySqlSslMode::VerifyIdentity,
            ssl_ca: Some("/tmp/ca #1.pem".to_string()),
            ssl_cert: Some(r"C:\certs\client.pem".to_string()),
            ssl_key: Some(r"C:\certs\client-key.pem".to_string()),
            ..mysql_fields(Some("app"))
        });

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        let config = profile.config.as_mysql().unwrap();
        assert_eq!(config.ssl_mode, MySqlSslMode::VerifyIdentity);
        assert_eq!(config.ssl_ca.as_deref(), Some("/tmp/ca #1.pem"));
        assert_eq!(config.ssl_cert.as_deref(), Some(r"C:\certs\client.pem"));
        assert_eq!(config.ssl_key.as_deref(), Some(r"C:\certs\client-key.pem"));

        let serialized = serialized_mysql_fields(&profile);
        assert_eq!(serialized.ssl_ca.as_deref(), Some("/tmp/ca #1.pem"));
        assert_eq!(serialized.ssl_cert.as_deref(), Some(r"C:\certs\client.pem"));
        assert_eq!(
            serialized.ssl_key.as_deref(),
            Some(r"C:\certs\client-key.pem")
        );
    }

    #[test]
    fn mysql_entry_round_trips_server_public_key_path() {
        let entry = mysql_entry(MySqlConfigEntry {
            server_public_key_path: Some(r"C:\keys\server-public.pem".to_string()),
            ..mysql_fields(Some("app"))
        });

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        assert_eq!(
            profile
                .config
                .as_mysql()
                .unwrap()
                .server_public_key_path
                .as_deref(),
            Some(r"C:\keys\server-public.pem")
        );

        assert_eq!(
            serialized_mysql_fields(&profile)
                .server_public_key_path
                .as_deref(),
            Some(r"C:\keys\server-public.pem")
        );
    }

    #[test]
    fn mysql_entry_drops_ca_for_non_verification_mode() {
        let entry = mysql_entry(MySqlConfigEntry {
            ssl_ca: Some("/tmp/old-ca.pem".to_string()),
            ..mysql_fields(Some("app"))
        });

        let profile = ConnectionProfile::try_from(&entry).unwrap();
        let config = profile.config.as_mysql().unwrap();
        assert_eq!(config.ssl_ca, None);

        assert_eq!(serialized_mysql_fields(&profile).ssl_ca, None);
    }

    #[test]
    fn mysql_entry_without_database_is_valid() {
        let profile = ConnectionProfile::try_from(&mysql_entry(mysql_fields(None))).unwrap();

        let config = profile.config.as_mysql().unwrap();
        assert_eq!(config.database, None);
        assert_eq!(config.ssl_mode, MySqlSslMode::Required);
    }

    #[test]
    fn mysql_entry_requires_host_and_username() {
        let without_host = mysql_entry(MySqlConfigEntry {
            host: None,
            ..mysql_fields(None)
        });
        assert!(matches!(
            ConnectionProfile::try_from(&without_host),
            Err(ConnectionProfileError::MissingMySqlField("host"))
        ));

        let blank_username = mysql_entry(MySqlConfigEntry {
            username: Some(" ".to_string()),
            ..mysql_fields(None)
        });
        assert!(matches!(
            ConnectionProfile::try_from(&blank_username),
            Err(ConnectionProfileError::MissingMySqlField("username"))
        ));
    }

    #[test]
    fn mysql_entry_rejects_invalid_host() {
        let entry = mysql_entry(MySqlConfigEntry {
            host: Some("db example".to_string()),
            ..mysql_fields(None)
        });

        assert!(matches!(
            ConnectionProfile::try_from(&entry),
            Err(ConnectionProfileError::InvalidMySqlHost)
        ));
    }

    #[test]
    fn mysql_entry_rejects_zero_port() {
        let entry = mysql_entry(MySqlConfigEntry {
            port: Some(0),
            ..mysql_fields(None)
        });

        assert!(matches!(
            ConnectionProfile::try_from(&entry),
            Err(ConnectionProfileError::InvalidMySqlPort)
        ));
    }
}
