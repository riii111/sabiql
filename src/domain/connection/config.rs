use serde::{Deserialize, Serialize};

use super::ssl_mode::SslMode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostgresConnectionConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
}

impl PostgresConnectionConfig {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            database: database.into(),
            username: username.into(),
            password: password.into(),
            ssl_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteConnectionConfig {
    pub path: String,
}

impl SqliteConnectionConfig {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionConfig {
    PostgreSQL(PostgresConnectionConfig),
    SQLite(SqliteConnectionConfig),
}

impl ConnectionConfig {
    pub fn database_type(&self) -> super::DatabaseType {
        match self {
            Self::PostgreSQL(_) => super::DatabaseType::PostgreSQL,
            Self::SQLite(_) => super::DatabaseType::SQLite,
        }
    }

    pub fn as_postgres(&self) -> Option<&PostgresConnectionConfig> {
        match self {
            Self::PostgreSQL(config) => Some(config),
            Self::SQLite(_) => None,
        }
    }

    pub fn as_sqlite(&self) -> Option<&SqliteConnectionConfig> {
        match self {
            Self::SQLite(config) => Some(config),
            Self::PostgreSQL(_) => None,
        }
    }
}
