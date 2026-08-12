use serde::{Deserialize, Serialize};
use url::Url;

use super::ssl_mode::SslMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MySqlSslMode {
    Disabled,
    #[default]
    Preferred,
    Required,
}

impl MySqlSslMode {
    pub const fn all_variants() -> &'static [Self] {
        &[Self::Disabled, Self::Preferred, Self::Required]
    }
}

impl std::fmt::Display for MySqlSslMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Disabled => "DISABLED",
            Self::Preferred => "PREFERRED",
            Self::Required => "REQUIRED",
        })
    }
}

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
pub struct MySqlConnectionConfig {
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub username: String,
    pub password: String,
    pub ssl_mode: MySqlSslMode,
}

impl MySqlConnectionConfig {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        database: Option<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: MySqlSslMode,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            database,
            username: username.into(),
            password: password.into(),
            ssl_mode,
        }
    }

    pub fn is_valid_host(host: &str) -> bool {
        let trimmed_host = host.trim();
        if trimmed_host.is_empty() || trimmed_host != host {
            return false;
        }
        let host = trimmed_host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let mut url = Url::parse("mysql://localhost").expect("static MySQL URL is valid");
        url.set_host(Some(&host)).is_ok()
    }

    pub fn is_valid(&self) -> bool {
        Self::is_valid_host(&self.host)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SqliteConnectionConfig {
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SqliteConnectionConfigError {
    #[error("SQLite database path is required")]
    EmptyPath,
    #[error("SQLite database path contains unsupported characters")]
    UnsupportedPath,
    #[error(
        "SQLite in-memory databases are not supported because sabiql starts sqlite3 per operation and cannot retain their contents; use a temporary database file"
    )]
    UnsupportedInMemoryDatabase,
    #[error("SQLite URI filenames are not supported; use a regular file path")]
    UnsupportedUriFilename,
}

impl SqliteConnectionConfig {
    pub fn new(path: impl Into<String>) -> Result<Self, SqliteConnectionConfigError> {
        let path = path.into();
        validate_sqlite_path(&path)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl<'de> Deserialize<'de> for SqliteConnectionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawSqliteConnectionConfig {
            path: String,
        }

        let raw = RawSqliteConnectionConfig::deserialize(deserializer)?;
        Self::new(raw.path).map_err(serde::de::Error::custom)
    }
}

fn validate_sqlite_path(path: &str) -> Result<(), SqliteConnectionConfigError> {
    if path.trim().is_empty() {
        return Err(SqliteConnectionConfigError::EmptyPath);
    }
    if path.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(SqliteConnectionConfigError::UnsupportedPath);
    }
    if path.trim() == ":memory:" {
        return Err(SqliteConnectionConfigError::UnsupportedInMemoryDatabase);
    }
    if is_sqlite_uri_filename(path) {
        return Err(SqliteConnectionConfigError::UnsupportedUriFilename);
    }
    Ok(())
}

fn is_sqlite_uri_filename(path: &str) -> bool {
    path.as_bytes()
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"file:"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionConfig {
    PostgreSQL(PostgresConnectionConfig),
    SQLite(SqliteConnectionConfig),
    MySQL(MySqlConnectionConfig),
}

impl ConnectionConfig {
    pub fn database_type(&self) -> super::DatabaseType {
        match self {
            Self::PostgreSQL(_) => super::DatabaseType::PostgreSQL,
            Self::SQLite(_) => super::DatabaseType::SQLite,
            Self::MySQL(_) => super::DatabaseType::MySQL,
        }
    }

    pub fn as_postgres(&self) -> Option<&PostgresConnectionConfig> {
        match self {
            Self::PostgreSQL(config) => Some(config),
            Self::SQLite(_) | Self::MySQL(_) => None,
        }
    }

    pub fn as_sqlite(&self) -> Option<&SqliteConnectionConfig> {
        match self {
            Self::SQLite(config) => Some(config),
            Self::PostgreSQL(_) | Self::MySQL(_) => None,
        }
    }

    pub fn as_mysql(&self) -> Option<&MySqlConnectionConfig> {
        match self {
            Self::MySQL(config) => Some(config),
            Self::PostgreSQL(_) | Self::SQLite(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sqlite_deserialize {
        use super::*;

        #[test]
        fn accepts_valid_path() {
            let config: SqliteConnectionConfig =
                serde_json::from_str(r#"{ "path": "/tmp/app.db" }"#).unwrap();

            assert_eq!(config.path(), "/tmp/app.db");
        }

        #[test]
        fn rejects_empty_path() {
            let result = serde_json::from_str::<SqliteConnectionConfig>(r#"{ "path": "   " }"#);

            assert!(result.is_err());
        }

        #[test]
        fn rejects_control_characters() {
            let result =
                serde_json::from_str::<SqliteConnectionConfig>(r#"{ "path": "/tmp/app\n.db" }"#);

            assert!(result.is_err());
        }

        #[test]
        fn rejects_in_memory_database() {
            let result =
                serde_json::from_str::<SqliteConnectionConfig>(r#"{ "path": ":memory:" }"#);

            assert!(matches!(
                result,
                Err(error) if error.to_string().contains("in-memory")
            ));
        }

        #[test]
        fn rejects_uri_filename() {
            let result = serde_json::from_str::<SqliteConnectionConfig>(
                r#"{ "path": "file:/tmp/app.db?mode=ro" }"#,
            );

            assert!(matches!(
                result,
                Err(error) if error.to_string().contains("URI filename")
            ));
        }

        #[test]
        fn rejects_uri_filename_case_insensitively() {
            let result =
                serde_json::from_str::<SqliteConnectionConfig>(r#"{ "path": "FILE:/tmp/app.db" }"#);

            assert!(result.is_err());
        }
    }

    mod mysql_host_validation {
        use super::*;

        #[test]
        fn accepts_dns_and_ipv6_hosts() {
            assert!(MySqlConnectionConfig::is_valid_host("localhost"));
            assert!(MySqlConnectionConfig::is_valid_host("db.example"));
            assert!(MySqlConnectionConfig::is_valid_host("::1"));
            assert!(MySqlConnectionConfig::is_valid_host("[::1]"));
        }

        #[test]
        fn rejects_url_syntax_in_host() {
            assert!(!MySqlConnectionConfig::is_valid_host("db example"));
            assert!(!MySqlConnectionConfig::is_valid_host("db/example"));
            assert!(!MySqlConnectionConfig::is_valid_host(" localhost "));
            assert!(!MySqlConnectionConfig::is_valid_host(
                "db?ssl-mode=REQUIRED"
            ));
        }
    }

    mod validate_sqlite_path {
        use super::*;

        #[test]
        fn accepts_regular_file_path() {
            assert!(validate_sqlite_path("/tmp/app.db").is_ok());
            assert!(validate_sqlite_path("./relative/app.db").is_ok());
        }

        #[test]
        fn rejects_memory_database() {
            assert!(matches!(
                validate_sqlite_path(":memory:"),
                Err(SqliteConnectionConfigError::UnsupportedInMemoryDatabase)
            ));
        }

        #[test]
        fn rejects_file_uri() {
            assert!(matches!(
                validate_sqlite_path("file:memdb?mode=memory"),
                Err(SqliteConnectionConfigError::UnsupportedUriFilename)
            ));
        }
    }
}
