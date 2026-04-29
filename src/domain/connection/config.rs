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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SqliteConnectionConfigError {
    #[error("SQLite database path is required")]
    EmptyPath,
    #[error("SQLite database path contains unsupported characters")]
    UnsupportedPath,
}

impl SqliteConnectionConfig {
    pub fn new(path: impl Into<String>) -> Result<Self, SqliteConnectionConfigError> {
        let path = path.into();
        validate_sqlite_path(&path)?;
        Ok(Self { path })
    }

    pub fn validate(&self) -> Result<(), SqliteConnectionConfigError> {
        validate_sqlite_path(&self.path)
    }
}

fn validate_sqlite_path(path: &str) -> Result<(), SqliteConnectionConfigError> {
    if path.trim().is_empty() {
        return Err(SqliteConnectionConfigError::EmptyPath);
    }
    if path.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(SqliteConnectionConfigError::UnsupportedPath);
    }
    Ok(())
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
