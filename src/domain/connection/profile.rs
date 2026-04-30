use serde::{Deserialize, Serialize};

use super::config::{
    ConnectionConfig, PostgresConnectionConfig, SqliteConnectionConfig, SqliteConnectionConfigError,
};
use super::database_type::DatabaseType;
use super::id::ConnectionId;
use super::name::{ConnectionName, ConnectionNameError};
use super::ssl_mode::SslMode;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionProfileError {
    #[error("{0}")]
    Name(#[from] ConnectionNameError),
    #[error("SQLite database path is required")]
    EmptySqlitePath,
    #[error("SQLite database path contains unsupported characters")]
    InvalidSqlitePath,
    #[error("PostgreSQL connection field `{0}` is required")]
    MissingPostgresField(&'static str),
}

impl From<SqliteConnectionConfigError> for ConnectionProfileError {
    fn from(error: SqliteConnectionConfigError) -> Self {
        match error {
            SqliteConnectionConfigError::EmptyPath => Self::EmptySqlitePath,
            SqliteConnectionConfigError::UnsupportedPath => Self::InvalidSqlitePath,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: ConnectionId,
    pub name: ConnectionName,
    pub config: ConnectionConfig,
}

impl ConnectionProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionNameError> {
        Self::new_postgres(name, host, port, database, username, password, ssl_mode).map_err(|e| {
            match e {
                ConnectionProfileError::Name(e) => e,
                ConnectionProfileError::EmptySqlitePath => unreachable!("postgres constructor"),
                ConnectionProfileError::InvalidSqlitePath => {
                    unreachable!("postgres constructor")
                }
                ConnectionProfileError::MissingPostgresField(_) => {
                    unreachable!("postgres constructor")
                }
            }
        })
    }

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

    pub fn with_id(
        id: ConnectionId,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionNameError> {
        Self::with_id_postgres(id, name, host, port, database, username, password, ssl_mode)
            .map_err(|e| match e {
                ConnectionProfileError::Name(e) => e,
                ConnectionProfileError::EmptySqlitePath => unreachable!("postgres constructor"),
                ConnectionProfileError::InvalidSqlitePath => {
                    unreachable!("postgres constructor")
                }
                ConnectionProfileError::MissingPostgresField(_) => {
                    unreachable!("postgres constructor")
                }
            })
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

    pub fn with_id_sqlite(
        id: ConnectionId,
        name: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<Self, ConnectionProfileError> {
        Ok(Self {
            id,
            name: ConnectionName::new(name)?,
            config: ConnectionConfig::SQLite(SqliteConnectionConfig::new(path)?),
        })
    }

    pub fn with_id_and_config(
        id: ConnectionId,
        name: impl Into<String>,
        config: ConnectionConfig,
    ) -> Result<Self, ConnectionProfileError> {
        if let ConnectionConfig::SQLite(sqlite) = &config {
            sqlite.validate()?;
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

    pub fn postgres_config(&self) -> Option<&PostgresConnectionConfig> {
        self.config.as_postgres()
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
        ConnectionProfile::new(
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
            let result = ConnectionProfile::new(
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
                Err(ConnectionProfileError::EmptySqlitePath)
            ));
        }

        #[test]
        fn sqlite_profile_rejects_unsupported_path_characters() {
            let result = ConnectionProfile::new_sqlite("Local", "/tmp/app\0.db");

            assert!(matches!(
                result,
                Err(ConnectionProfileError::InvalidSqlitePath)
            ));
        }
    }
}
