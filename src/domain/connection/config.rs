use serde::{Deserialize, Serialize};
use std::str::FromStr;
use url::Url;

use super::ssl_mode::SslMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MySqlTransport {
    #[default]
    Tcp,
    #[serde(rename = "UNIX_SOCKET")]
    UnixSocket,
    #[serde(rename = "NAMED_PIPE")]
    NamedPipe,
}

impl MySqlTransport {
    pub const fn all_variants() -> &'static [Self] {
        #[cfg(unix)]
        {
            &[Self::Tcp, Self::UnixSocket]
        }
        #[cfg(windows)]
        {
            &[Self::Tcp, Self::NamedPipe]
        }
        #[cfg(not(any(unix, windows)))]
        {
            &[Self::Tcp]
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::UnixSocket => "UNIX_SOCKET",
            Self::NamedPipe => "NAMED_PIPE",
        }
    }

    pub const fn protocol(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::UnixSocket => "SOCKET",
            Self::NamedPipe => "PIPE",
        }
    }

    pub const fn requires_path(self) -> bool {
        !matches!(self, Self::Tcp)
    }

    pub const fn is_supported_on_current_platform(self) -> bool {
        match self {
            Self::Tcp => true,
            #[cfg(unix)]
            Self::UnixSocket => true,
            #[cfg(not(unix))]
            Self::UnixSocket => false,
            #[cfg(windows)]
            Self::NamedPipe => true,
            #[cfg(not(windows))]
            Self::NamedPipe => false,
        }
    }
}

impl std::fmt::Display for MySqlTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MySqlTransport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TCP" => Ok(Self::Tcp),
            "UNIX_SOCKET" => Ok(Self::UnixSocket),
            "NAMED_PIPE" => Ok(Self::NamedPipe),
            _ => Err(format!("Unknown MySQL transport: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MySqlSslMode {
    Disabled,
    #[default]
    Preferred,
    Required,
    #[serde(rename = "VERIFY_CA")]
    VerifyCa,
    #[serde(rename = "VERIFY_IDENTITY")]
    VerifyIdentity,
}

impl MySqlSslMode {
    pub const fn all_variants() -> &'static [Self] {
        &[
            Self::Disabled,
            Self::Preferred,
            Self::Required,
            Self::VerifyCa,
            Self::VerifyIdentity,
        ]
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "DISABLED",
            Self::Preferred => "PREFERRED",
            Self::Required => "REQUIRED",
            Self::VerifyCa => "VERIFY_CA",
            Self::VerifyIdentity => "VERIFY_IDENTITY",
        }
    }

    pub const fn uses_ca(self) -> bool {
        matches!(self, Self::VerifyCa | Self::VerifyIdentity)
    }

    pub const fn allows_cleartext_auth(self) -> bool {
        matches!(self, Self::Required | Self::VerifyCa | Self::VerifyIdentity)
    }
}

impl std::fmt::Display for MySqlSslMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MySqlSslMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "DISABLED" => Ok(Self::Disabled),
            "PREFERRED" => Ok(Self::Preferred),
            "REQUIRED" => Ok(Self::Required),
            "VERIFY_CA" => Ok(Self::VerifyCa),
            "VERIFY_IDENTITY" => Ok(Self::VerifyIdentity),
            _ => Err(format!("Unknown MySQL SSL mode: {s}")),
        }
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
    #[serde(default)]
    pub transport: MySqlTransport,
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub username: String,
    pub password: String,
    pub ssl_mode: MySqlSslMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_ca: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_public_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub enable_cleartext_plugin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_path: Option<String>,
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
            transport: MySqlTransport::Tcp,
            host: host.into(),
            port,
            database,
            username: username.into(),
            password: password.into(),
            ssl_mode,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
            server_public_key_path: None,
            enable_cleartext_plugin: false,
            transport_path: None,
        }
    }

    #[must_use]
    pub fn with_tls_paths(
        mut self,
        ssl_ca: Option<String>,
        ssl_cert: Option<String>,
        ssl_key: Option<String>,
    ) -> Self {
        self.ssl_ca = self.ssl_mode.uses_ca().then_some(ssl_ca).flatten();
        self.ssl_cert = ssl_cert;
        self.ssl_key = ssl_key;
        self
    }

    #[must_use]
    pub fn with_server_public_key_path(mut self, path: Option<String>) -> Self {
        self.server_public_key_path = path;
        self
    }

    #[must_use]
    pub fn with_cleartext_auth_plugin(mut self, enabled: bool) -> Self {
        self.enable_cleartext_plugin = enabled;
        self
    }

    #[must_use]
    pub fn with_transport(mut self, transport: MySqlTransport, path: Option<String>) -> Self {
        self.transport = transport;
        self.transport_path = transport.requires_path().then_some(path).flatten();
        self
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
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if requires a reference predicate"
)]
fn is_false(value: &bool) -> bool {
    !*value
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

    #[test]
    fn mysql_tls_modes_use_mysql_option_names() {
        let expected = [
            (MySqlSslMode::Disabled, "\"DISABLED\""),
            (MySqlSslMode::Preferred, "\"PREFERRED\""),
            (MySqlSslMode::Required, "\"REQUIRED\""),
            (MySqlSslMode::VerifyCa, "\"VERIFY_CA\""),
            (MySqlSslMode::VerifyIdentity, "\"VERIFY_IDENTITY\""),
        ];

        for (mode, serialized) in expected {
            assert_eq!(serde_json::to_string(&mode).unwrap(), serialized);
        }
    }

    #[test]
    fn mysql_tls_modes_round_trip_through_canonical_names() {
        for mode in MySqlSslMode::all_variants() {
            assert_eq!(mode.as_str(), mode.to_string());
            assert_eq!(mode.as_str().parse::<MySqlSslMode>().unwrap(), *mode);
        }
    }

    #[test]
    fn mysql_ca_is_only_kept_for_certificate_verification_modes() {
        for mode in [
            MySqlSslMode::Disabled,
            MySqlSslMode::Preferred,
            MySqlSslMode::Required,
        ] {
            let config =
                MySqlConnectionConfig::new("localhost", 3306, None, "user", "password", mode)
                    .with_tls_paths(Some("/tmp/ca.pem".to_string()), None, None);

            assert_eq!(config.ssl_ca, None);
        }

        for mode in [MySqlSslMode::VerifyCa, MySqlSslMode::VerifyIdentity] {
            let config =
                MySqlConnectionConfig::new("localhost", 3306, None, "user", "password", mode)
                    .with_tls_paths(Some("/tmp/ca.pem".to_string()), None, None);

            assert_eq!(config.ssl_ca.as_deref(), Some("/tmp/ca.pem"));
        }
    }

    #[test]
    fn mysql_tls_modes_reject_unknown_names() {
        assert!("unknown".parse::<MySqlSslMode>().is_err());
    }

    #[test]
    fn mysql_transports_round_trip_through_canonical_names() {
        let expected = [
            (MySqlTransport::Tcp, "TCP"),
            (MySqlTransport::UnixSocket, "UNIX_SOCKET"),
            (MySqlTransport::NamedPipe, "NAMED_PIPE"),
        ];

        for (transport, name) in expected {
            assert_eq!(transport.as_str(), name);
            assert_eq!(name.parse::<MySqlTransport>().unwrap(), transport);
            assert_eq!(
                serde_json::to_string(&transport).unwrap(),
                format!("\"{name}\"")
            );
        }
    }

    #[test]
    fn cleartext_auth_requires_a_tls_mode() {
        assert!(!MySqlSslMode::Disabled.allows_cleartext_auth());
        assert!(!MySqlSslMode::Preferred.allows_cleartext_auth());
        assert!(MySqlSslMode::Required.allows_cleartext_auth());
        assert!(MySqlSslMode::VerifyCa.allows_cleartext_auth());
        assert!(MySqlSslMode::VerifyIdentity.allows_cleartext_auth());
    }

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
