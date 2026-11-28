use serde::{Deserialize, Serialize};

use super::config::{
    ConnectionConfig, MySqlConnectionConfig, MySqlTransport, SqliteConnectionConfig,
    SqliteConnectionConfigError,
};
#[cfg(any(test, feature = "test-support"))]
use super::config::{MySqlSslMode, PostgresConnectionConfig};
use super::database_type::DatabaseType;
use super::id::ConnectionId;
use super::name::{ConnectionName, ConnectionNameError};
use super::sqlite_path::SqlitePathError;
#[cfg(any(test, feature = "test-support"))]
use super::ssl_mode::SslMode;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionProfileError {
    #[error("{0}")]
    Name(#[from] ConnectionNameError),
    #[error(transparent)]
    SqliteConfig(#[from] SqliteConnectionConfigError),
    #[error("PostgreSQL connection field `{0}` is required")]
    MissingPostgresField(&'static str),
    #[error("MySQL connection field `{0}` is required")]
    MissingMySqlField(&'static str),
    #[error("MySQL connection host is invalid")]
    InvalidMySqlHost,
    #[error("MySQL connection port must be > 0")]
    InvalidMySqlPort,
    #[error("MySQL connection transport is not supported on this platform")]
    UnsupportedMySqlTransport,
    #[error("MySQL connection transport path is required")]
    MissingMySqlTransportPath,
    #[error("MySQL connection transport path is invalid")]
    InvalidMySqlTransportPath,
    #[error("MySQL named pipe transport does not support the selected TLS mode")]
    MySqlNamedPipeRequiresNonTls,
    #[error(
        "MySQL cleartext authentication requires REQUIRED, VERIFY_CA, or VERIFY_IDENTITY TLS mode"
    )]
    MySqlCleartextAuthRequiresTls,
    #[error("{0}")]
    SqlitePath(#[from] SqlitePathError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: ConnectionId,
    pub name: ConnectionName,
    pub config: ConnectionConfig,
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectionProfile {
    pub fn new_postgres(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionProfileError> {
        Ok(Self {
            id: ConnectionId::new(),
            name: ConnectionName::new(name)?,
            config: ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                host, port, database, username, password, ssl_mode,
            )),
        })
    }

    pub fn new_sqlite(
        name: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<Self, ConnectionProfileError> {
        Ok(Self {
            id: ConnectionId::new(),
            name: ConnectionName::new(name)?,
            config: ConnectionConfig::SQLite(SqliteConnectionConfig::new(path)?),
        })
    }

    pub fn new_mysql(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: Option<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: MySqlSslMode,
    ) -> Result<Self, ConnectionProfileError> {
        Self::with_id_and_config(
            ConnectionId::new(),
            name,
            ConnectionConfig::MySQL(MySqlConnectionConfig::new(
                host, port, database, username, password, ssl_mode,
            )),
        )
    }

    pub fn with_id_postgres(
        id: ConnectionId,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionProfileError> {
        Ok(Self {
            id,
            name: ConnectionName::new(name)?,
            config: ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                host, port, database, username, password, ssl_mode,
            )),
        })
    }
}

impl ConnectionProfile {
    pub fn with_id_and_config(
        id: ConnectionId,
        name: impl Into<String>,
        config: ConnectionConfig,
    ) -> Result<Self, ConnectionProfileError> {
        match &config {
            ConnectionConfig::MySQL(mysql) => {
                if !mysql.transport.is_supported_on_current_platform() {
                    return Err(ConnectionProfileError::UnsupportedMySqlTransport);
                }
                match mysql.transport {
                    MySqlTransport::Tcp => {
                        if mysql.port == 0 {
                            return Err(ConnectionProfileError::InvalidMySqlPort);
                        }
                        if !MySqlConnectionConfig::is_valid_host(&mysql.host) {
                            return Err(ConnectionProfileError::InvalidMySqlHost);
                        }
                    }
                    MySqlTransport::UnixSocket | MySqlTransport::NamedPipe => {
                        let path = mysql
                            .transport_path
                            .as_deref()
                            .ok_or(ConnectionProfileError::MissingMySqlTransportPath)?;
                        if !MySqlTransport::is_valid_path(path) {
                            return Err(ConnectionProfileError::InvalidMySqlTransportPath);
                        }
                        if !mysql.transport.allows_ssl_mode(mysql.ssl_mode) {
                            return Err(ConnectionProfileError::MySqlNamedPipeRequiresNonTls);
                        }
                    }
                }
                if mysql.enable_cleartext_plugin && !mysql.ssl_mode.allows_cleartext_auth() {
                    return Err(ConnectionProfileError::MySqlCleartextAuthRequiresTls);
                }
            }
            ConnectionConfig::PostgreSQL(_) | ConnectionConfig::SQLite(_) => {}
        }
        Ok(Self {
            id,
            name: ConnectionName::new(name)?,
            config,
        })
    }

    pub fn database_type(&self) -> DatabaseType {
        self.config.database_type()
    }

    pub fn sqlite_config(&self) -> Option<&SqliteConnectionConfig> {
        self.config.as_sqlite()
    }

    pub fn display_name(&self) -> &str {
        self.name.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_profile() -> ConnectionProfile {
        ConnectionProfile::new_postgres(
            "Test Connection",
            "localhost",
            5432,
            "testdb",
            "testuser",
            "testpass",
            SslMode::Prefer,
        )
        .unwrap()
    }

    mod new {
        use super::*;

        #[test]
        fn generates_unique_id() {
            let p1 = make_test_profile();
            let p2 = make_test_profile();
            assert_ne!(p1.id, p2.id);
        }

        #[test]
        fn empty_name_returns_error() {
            let result = ConnectionProfile::new_postgres(
                "",
                "localhost",
                5432,
                "testdb",
                "testuser",
                "testpass",
                SslMode::Prefer,
            );
            assert!(result.is_err());
        }

        #[test]
        fn invalid_mysql_host_returns_error() {
            let result = ConnectionProfile::new_mysql(
                "MySQL",
                "db example",
                3306,
                None,
                "user",
                "password",
                MySqlSslMode::Preferred,
            );

            assert!(matches!(
                result,
                Err(ConnectionProfileError::InvalidMySqlHost)
            ));
        }

        #[test]
        fn zero_mysql_port_returns_error() {
            let result = ConnectionProfile::new_mysql(
                "MySQL",
                "localhost",
                0,
                None,
                "user",
                "password",
                MySqlSslMode::Preferred,
            );

            assert!(matches!(
                result,
                Err(ConnectionProfileError::InvalidMySqlPort)
            ));
        }

        #[cfg(unix)]
        #[test]
        fn unix_socket_profile_requires_a_valid_path() {
            let config = MySqlConnectionConfig::new(
                "ignored-host",
                3306,
                None,
                "user",
                "password",
                MySqlSslMode::Disabled,
            )
            .with_transport(MySqlTransport::UnixSocket, None);
            let result = ConnectionProfile::with_id_and_config(
                ConnectionId::new(),
                "MySQL",
                ConnectionConfig::MySQL(config),
            );
            assert!(matches!(
                result,
                Err(ConnectionProfileError::MissingMySqlTransportPath)
            ));

            let config = MySqlConnectionConfig::new(
                "ignored-host",
                3306,
                None,
                "user",
                "password",
                MySqlSslMode::Disabled,
            )
            .with_transport(MySqlTransport::UnixSocket, Some("\n".to_string()));
            let result = ConnectionProfile::with_id_and_config(
                ConnectionId::new(),
                "MySQL",
                ConnectionConfig::MySQL(config),
            );
            assert!(matches!(
                result,
                Err(ConnectionProfileError::InvalidMySqlTransportPath)
            ));
        }

        #[test]
        fn cleartext_auth_requires_tls() {
            let config = MySqlConnectionConfig::new(
                "localhost",
                3306,
                None,
                "user",
                "password",
                MySqlSslMode::Preferred,
            )
            .with_cleartext_auth_plugin(true);

            let result = ConnectionProfile::with_id_and_config(
                ConnectionId::new(),
                "MySQL",
                ConnectionConfig::MySQL(config),
            );

            assert!(matches!(
                result,
                Err(ConnectionProfileError::MySqlCleartextAuthRequiresTls)
            ));
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn formats_connection_name() {
            let profile = make_test_profile();
            assert_eq!(profile.display_name(), "Test Connection");
        }
    }

    mod database_type {
        use super::*;

        #[test]
        fn postgres_profile_reports_postgresql() {
            let profile = make_test_profile();

            assert_eq!(profile.database_type(), DatabaseType::PostgreSQL);
        }

        #[test]
        fn sqlite_profile_reports_sqlite() {
            let profile = ConnectionProfile::new_sqlite("Local", "/tmp/app.db").unwrap();

            assert_eq!(profile.database_type(), DatabaseType::SQLite);
        }

        #[test]
        fn sqlite_profile_rejects_empty_path() {
            let result = ConnectionProfile::new_sqlite("Local", " ");

            assert!(matches!(
                result,
                Err(ConnectionProfileError::SqliteConfig(
                    SqliteConnectionConfigError::EmptyPath
                ))
            ));
        }

        #[test]
        fn sqlite_profile_rejects_unsupported_path_characters() {
            let result = ConnectionProfile::new_sqlite("Local", "/tmp/app\0.db");

            assert!(matches!(
                result,
                Err(ConnectionProfileError::SqliteConfig(
                    SqliteConnectionConfigError::UnsupportedPath
                ))
            ));
        }

        #[test]
        fn sqlite_profile_rejects_in_memory_database() {
            let result = ConnectionProfile::new_sqlite("Local", ":memory:");

            assert!(matches!(
                result,
                Err(ConnectionProfileError::SqliteConfig(
                    SqliteConnectionConfigError::UnsupportedInMemoryDatabase
                ))
            ));
        }

        #[test]
        fn sqlite_profile_rejects_uri_filename() {
            let result = ConnectionProfile::new_sqlite("Local", "file:/tmp/app.db?mode=ro");

            assert!(matches!(
                result,
                Err(ConnectionProfileError::SqliteConfig(
                    SqliteConnectionConfigError::UnsupportedUriFilename
                ))
            ));
        }
    }
}
