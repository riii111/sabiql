use crate::policy::password_masking::mask_password;
use crate::ports::outbound::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, SQLITE_SAFE_MODE_REQUIRED_MARKER,
    SQLITE_TABLE_LIST_REQUIRED_MARKER, UnsupportedOperationKind, is_mysql_connect_timeout_message,
    mysql_server_error_code,
};
use sabiql_domain::connection::MySqlSslMode;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionErrorKind {
    CliNotFound,
    MySqlCliNotFound,
    MySqlCliVersionUnsupported,
    MySqlServerVersionUnsupported,
    MySqlSqlModeUnsupported,
    MySqlTlsHandshakeFailed,
    MySqlCaVerificationFailed,
    MySqlHostnameVerificationFailed,
    MySqlClientCertificateRejected,
    SqliteCliNotFound,
    HostUnreachable,
    AuthFailed,
    PermissionDenied,
    DatabaseNotFound,
    ConnectionLost,
    Timeout,
    ConnectionRefused,
    SqliteVersionTooOld,
    SqliteFileNotFound,
    SqlitePathIsDirectory,
    SqlitePathNotRegularFile,
    SqliteNotDatabaseFile,
    SqliteReadAccessDenied,
    SqlitePathAccessDenied,
    SqlitePathIo,
    #[default]
    Unknown,
}

impl ConnectionErrorKind {
    pub fn classify(stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();

        if stderr_lower.contains("command not found")
            || stderr_lower.contains("not found: psql")
            || stderr_lower.contains("not found: mysql")
            || stderr_lower.contains("not recognized")
        {
            return Self::CliNotFound;
        }

        if stderr_lower.contains("could not translate host name")
            || stderr_lower.contains("name or service not known")
            || stderr_lower.contains("nodename nor servname provided")
            || stderr_lower.contains("no such host")
            || stderr_lower.contains("unknown mysql server host")
        {
            return Self::HostUnreachable;
        }

        if let Some(error_code) = mysql_server_error_code(&stderr_lower) {
            match error_code {
                1044 => return Self::PermissionDenied,
                1045 => return Self::AuthFailed,
                1049 => return Self::DatabaseNotFound,
                2003 => {}
                2006 | 2013 => return Self::ConnectionLost,
                _ => return Self::Unknown,
            }
        }

        if stderr_lower.contains("password authentication failed")
            || stderr_lower.contains("authentication failed")
            || stderr_lower.contains("access denied for user")
            || (stderr_lower.contains("fatal:") && stderr_lower.contains("password"))
        {
            return Self::AuthFailed;
        }

        if stderr_lower.contains("does not exist")
            && (stderr_lower.contains("database") || stderr_lower.contains("fatal:"))
        {
            return Self::DatabaseNotFound;
        }

        if is_mysql_connect_timeout_message(stderr)
            || stderr_lower.contains("timeout expired")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("connection timed out")
        {
            return Self::Timeout;
        }

        if stderr_lower.contains("connection refused")
            || stderr_lower.contains("can't connect to mysql server")
        {
            return Self::ConnectionRefused;
        }

        if is_connection_lost_message(&stderr_lower) {
            return Self::ConnectionLost;
        }

        Self::Unknown
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::CliNotFound => "Database CLI not found",
            Self::MySqlCliNotFound => DatabaseCli::MySql.not_found_summary(),
            Self::MySqlCliVersionUnsupported => "Unsupported MySQL CLI version",
            Self::MySqlServerVersionUnsupported => "Unsupported MySQL server version",
            Self::MySqlSqlModeUnsupported => "Unsupported MySQL sql_mode",
            Self::MySqlTlsHandshakeFailed => "MySQL TLS handshake failed",
            Self::MySqlCaVerificationFailed => "MySQL server certificate could not be verified",
            Self::MySqlHostnameVerificationFailed => "MySQL server hostname could not be verified",
            Self::MySqlClientCertificateRejected => "MySQL client certificate was rejected",
            Self::SqliteCliNotFound => DatabaseCli::Sqlite3.not_found_summary(),
            Self::HostUnreachable => "Could not resolve host",
            Self::AuthFailed => "Authentication failed",
            Self::PermissionDenied => "Permission denied",
            Self::DatabaseNotFound => "Database does not exist",
            Self::ConnectionLost => "Connection lost during operation",
            Self::Timeout => "Connection timed out",
            Self::ConnectionRefused => "Connection refused",
            Self::SqliteVersionTooOld => "SQLite 3.41.1 or later required",
            Self::SqliteFileNotFound => "SQLite database file not found",
            Self::SqlitePathIsDirectory => "SQLite path is a directory",
            Self::SqlitePathNotRegularFile => "SQLite path is not a regular file",
            Self::SqliteNotDatabaseFile => "File is not a SQLite database",
            Self::SqliteReadAccessDenied => "Cannot read SQLite database file",
            Self::SqlitePathAccessDenied => "Cannot access SQLite database file",
            Self::SqlitePathIo => "Cannot open SQLite database file",
            Self::Unknown => "Connection failed",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::CliNotFound => "Install the database CLI (e.g. psql) and add it to PATH",
            Self::MySqlCliNotFound => DatabaseCli::MySql.not_found_hint(),
            Self::MySqlCliVersionUnsupported => "Install the Oracle MySQL 8.4 client",
            Self::MySqlServerVersionUnsupported => "Connect to an Oracle MySQL 8.4 server",
            Self::MySqlSqlModeUnsupported => {
                "Disable NO_BACKSLASH_ESCAPES and ANSI_QUOTES for this connection"
            }
            Self::MySqlTlsHandshakeFailed => {
                "Check that the server and client support the selected TLS settings"
            }
            Self::MySqlCaVerificationFailed => {
                "Check the CA certificate path and server certificate"
            }
            Self::MySqlHostnameVerificationFailed => {
                "Use the hostname covered by the server certificate"
            }
            Self::MySqlClientCertificateRejected => {
                "Check the client certificate, key, and server account requirements"
            }
            Self::SqliteCliNotFound => DatabaseCli::Sqlite3.not_found_hint(),
            Self::HostUnreachable => "Check the hostname",
            Self::AuthFailed => "Check username and password",
            Self::PermissionDenied => "Check the connected user's privileges",
            Self::DatabaseNotFound => "Check database name",
            Self::ConnectionLost => "Reconnect and retry the operation",
            Self::Timeout => "Check network connectivity",
            Self::ConnectionRefused => "Check the host, port, and server availability",
            Self::SqliteVersionTooOld => "Upgrade sqlite3 to use SQLite safely",
            Self::SqliteFileNotFound => {
                "Check the file path — sabiql does not create new database files"
            }
            Self::SqlitePathIsDirectory => "Enter a path to a database file, not a folder",
            Self::SqlitePathNotRegularFile => {
                "Enter a path to a regular database file, not a pipe or special file"
            }
            Self::SqliteNotDatabaseFile => {
                "Choose a readable SQLite database file, or create one with sqlite3"
            }
            Self::SqliteReadAccessDenied => "Check read permissions for the database file",
            Self::SqlitePathAccessDenied => "Check file permissions for the database file",
            Self::SqlitePathIo => "Check that the database file path is valid and accessible",
            Self::Unknown => "See details for more information",
        }
    }

    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::HostUnreachable | Self::ConnectionLost | Self::Timeout | Self::ConnectionRefused
        )
    }
}

fn mysql_ssl_mode_from_dsn(dsn: &str) -> Option<MySqlSslMode> {
    let url = Url::parse(dsn).ok()?;
    if url.scheme() != "mysql" {
        return None;
    }
    url.query_pairs().find_map(|(key, value)| {
        if key != "ssl-mode" {
            return None;
        }
        value.parse().ok()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionErrorInfo {
    pub kind: ConnectionErrorKind,
    masked_details: String,
}

impl ConnectionErrorInfo {
    pub fn with_kind(kind: ConnectionErrorKind, raw_stderr: impl Into<String>) -> Self {
        let raw_details = raw_stderr.into();
        let masked_details = mask_password(&raw_details);

        Self {
            kind,
            masked_details,
        }
    }

    pub fn from_db_operation_error(error: &DbOperationError) -> Self {
        let raw_details = error.raw_details().into_owned();
        let kind = match error {
            DbOperationError::CommandNotFound {
                command: DatabaseCli::Sqlite3,
                ..
            } => ConnectionErrorKind::SqliteCliNotFound,
            DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                ..
            } => ConnectionErrorKind::MySqlCliNotFound,
            DbOperationError::CommandNotFound { .. } => ConnectionErrorKind::CliNotFound,
            DbOperationError::ConnectionLost(_) => ConnectionErrorKind::ConnectionLost,
            DbOperationError::Timeout(_) => ConnectionErrorKind::Timeout,
            DbOperationError::PermissionDenied(_) => ConnectionErrorKind::PermissionDenied,
            DbOperationError::UnsupportedOperationWithKind { kind, .. } => match kind {
                UnsupportedOperationKind::ClientVersion => {
                    ConnectionErrorKind::MySqlCliVersionUnsupported
                }
                UnsupportedOperationKind::ServerVersion => {
                    ConnectionErrorKind::MySqlServerVersionUnsupported
                }
                UnsupportedOperationKind::SessionMode => {
                    ConnectionErrorKind::MySqlSqlModeUnsupported
                }
            },
            DbOperationError::UnsupportedOperation(details)
                if details.contains(SQLITE_TABLE_LIST_REQUIRED_MARKER)
                    || details.contains(SQLITE_SAFE_MODE_REQUIRED_MARKER) =>
            {
                ConnectionErrorKind::SqliteVersionTooOld
            }
            DbOperationError::ConnectionFailedWithKind { kind, .. } => match kind {
                ConnectionFailureKind::TlsHandshake
                | ConnectionFailureKind::TlsCertificateVerification => {
                    ConnectionErrorKind::MySqlTlsHandshakeFailed
                }
                ConnectionFailureKind::TlsCaVerification => {
                    ConnectionErrorKind::MySqlCaVerificationFailed
                }
                ConnectionFailureKind::TlsHostnameVerification => {
                    ConnectionErrorKind::MySqlHostnameVerificationFailed
                }
                ConnectionFailureKind::TlsClientCertificateRejected => {
                    ConnectionErrorKind::MySqlClientCertificateRejected
                }
            },
            DbOperationError::ConnectionFailed(details) => {
                classify_sqlite_path_connection_error(details)
                    .unwrap_or_else(|| ConnectionErrorKind::classify(&raw_details))
            }
            _ => ConnectionErrorKind::Unknown,
        };
        Self::with_kind(kind, raw_details)
    }

    pub fn from_db_operation_error_with_dsn(error: &DbOperationError, dsn: &str) -> Self {
        let raw_details = error.raw_details().into_owned();
        let kind = mysql_ssl_mode_from_dsn(dsn)
            .and_then(|ssl_mode| match (ssl_mode, error) {
                (
                    MySqlSslMode::VerifyCa,
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::TlsCertificateVerification,
                        ..
                    },
                ) => Some(ConnectionErrorKind::MySqlCaVerificationFailed),
                (
                    MySqlSslMode::VerifyIdentity,
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::TlsCertificateVerification,
                        ..
                    },
                ) => Some(ConnectionErrorKind::MySqlHostnameVerificationFailed),
                _ => None,
            })
            .unwrap_or_else(|| Self::from_db_operation_error(error).kind);

        Self::with_kind(kind, raw_details)
    }

    pub fn masked_details(&self) -> &str {
        &self.masked_details
    }

    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
}

fn classify_sqlite_path_connection_error(message: &str) -> Option<ConnectionErrorKind> {
    use crate::domain::SqlitePathError;
    use crate::policy::sqlite_path::connection_error_kind;

    SqlitePathError::from_display_message(message).map(|error| connection_error_kind(&error))
}

fn is_connection_lost_message(lower: &str) -> bool {
    lower.contains("server closed the connection unexpectedly")
        || lower.contains("connection to server was lost")
        || lower.contains("terminating connection")
        || lower.contains("connection not open")
        || lower.contains("broken pipe")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod classify {
        use super::*;

        #[rstest]
        #[case("psql: command not found")]
        #[case("/bin/sh: psql: command not found")]
        #[case("zsh: command not found: psql")]
        #[case("not found: mysql")]
        fn stderr_as_cli_not_found(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::CliNotFound
            );
        }

        #[rstest]
        #[case(r#"psql: error: could not translate host name "host" to address: nodename nor servname provided"#)]
        #[case(r#"psql: error: could not translate host name "host" to address: Name or service not known"#)]
        fn stderr_as_host_unreachable(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::HostUnreachable
            );
        }

        #[rstest]
        #[case(r#"FATAL: password authentication failed for user "user""#)]
        #[case(r"psql: error: FATAL:  password authentication failed")]
        fn stderr_as_auth_failed(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::AuthFailed
            );
        }

        #[rstest]
        #[case(
            "ERROR 1044 (42000): Access denied for user 'user' to database 'mysql'",
            ConnectionErrorKind::PermissionDenied
        )]
        #[case(
            "ERROR 1045 (28000): Access denied for user 'user'",
            ConnectionErrorKind::AuthFailed
        )]
        #[case(
            "ERROR 1049 (42000): Unknown database 'missing'",
            ConnectionErrorKind::DatabaseNotFound
        )]
        fn mysql_server_error_codes_use_specific_connection_guidance(
            #[case] stderr: &str,
            #[case] expected: ConnectionErrorKind,
        ) {
            assert_eq!(ConnectionErrorKind::classify(stderr), expected);
        }

        #[test]
        fn unknown_mysql_server_error_code_fails_closed() {
            assert_eq!(
                ConnectionErrorKind::classify("ERROR 9999 (HY000): Access denied for user 'user'"),
                ConnectionErrorKind::Unknown
            );
        }

        #[test]
        fn stderr_as_database_not_found() {
            assert_eq!(
                ConnectionErrorKind::classify(r#"FATAL: database "nonexistent" does not exist"#),
                ConnectionErrorKind::DatabaseNotFound
            );
        }

        #[rstest]
        #[case("psql: error: timeout expired")]
        #[case("Connection timed out")]
        fn stderr_as_timeout(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Timeout
            );
        }

        #[rstest]
        #[case("psql: error: connection to server was lost")]
        #[case("server closed the connection unexpectedly")]
        fn stderr_as_connection_lost(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::ConnectionLost
            );
        }

        #[rstest]
        #[case("Some random error")]
        #[case("")]
        fn stderr_as_unknown_fallback(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Unknown
            );
        }

        #[test]
        fn stderr_as_connection_refused() {
            assert_eq!(
                ConnectionErrorKind::classify("Can't connect to MySQL server on 'localhost' (111)"),
                ConnectionErrorKind::ConnectionRefused
            );
        }

        #[rstest]
        #[case("ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (110)")]
        #[case("ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (10060)")]
        #[case("ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (60)")]
        fn stderr_as_mysql_timeout(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Timeout
            );
        }
    }

    mod error_kind {
        use super::*;

        #[rstest]
        #[case(ConnectionErrorKind::CliNotFound)]
        #[case(ConnectionErrorKind::SqliteCliNotFound)]
        #[case(ConnectionErrorKind::HostUnreachable)]
        #[case(ConnectionErrorKind::AuthFailed)]
        #[case(ConnectionErrorKind::PermissionDenied)]
        #[case(ConnectionErrorKind::DatabaseNotFound)]
        #[case(ConnectionErrorKind::ConnectionLost)]
        #[case(ConnectionErrorKind::Timeout)]
        #[case(ConnectionErrorKind::ConnectionRefused)]
        #[case(ConnectionErrorKind::MySqlCliNotFound)]
        #[case(ConnectionErrorKind::MySqlCliVersionUnsupported)]
        #[case(ConnectionErrorKind::MySqlServerVersionUnsupported)]
        #[case(ConnectionErrorKind::MySqlSqlModeUnsupported)]
        #[case(ConnectionErrorKind::MySqlTlsHandshakeFailed)]
        #[case(ConnectionErrorKind::MySqlCaVerificationFailed)]
        #[case(ConnectionErrorKind::MySqlHostnameVerificationFailed)]
        #[case(ConnectionErrorKind::MySqlClientCertificateRejected)]
        #[case(ConnectionErrorKind::SqliteVersionTooOld)]
        #[case(ConnectionErrorKind::SqliteFileNotFound)]
        #[case(ConnectionErrorKind::SqlitePathIsDirectory)]
        #[case(ConnectionErrorKind::SqlitePathNotRegularFile)]
        #[case(ConnectionErrorKind::SqliteNotDatabaseFile)]
        #[case(ConnectionErrorKind::SqliteReadAccessDenied)]
        #[case(ConnectionErrorKind::SqlitePathAccessDenied)]
        #[case(ConnectionErrorKind::SqlitePathIo)]
        #[case(ConnectionErrorKind::Unknown)]
        fn has_non_empty_summary_and_hint(#[case] kind: ConnectionErrorKind) {
            assert!(!kind.summary().is_empty());
            assert!(!kind.hint().is_empty());
        }
    }

    mod error_info {
        use super::*;

        #[test]
        fn with_kind_uses_provided_kind() {
            let info = ConnectionErrorInfo::with_kind(ConnectionErrorKind::Timeout, "error");
            assert_eq!(info.kind, ConnectionErrorKind::Timeout);
        }

        #[test]
        fn from_db_operation_error_classifies_from_raw_details() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    r#"FATAL: database "nonexistent" does not exist"#.to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::DatabaseNotFound);
            assert_eq!(
                info.masked_details(),
                "FATAL: database \"nonexistent\" does not exist"
            );
        }

        #[test]
        fn from_db_operation_error_with_dsn_uses_mysql_tls_mode_for_ambiguous_verification() {
            let error = DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::TlsCertificateVerification,
                details: "certificate verification failed".to_string(),
            };
            let ca = ConnectionErrorInfo::from_db_operation_error_with_dsn(
                &error,
                "mysql://user:password@localhost:3306/app?ssl-mode=VERIFY_CA",
            );
            let identity = ConnectionErrorInfo::from_db_operation_error_with_dsn(
                &error,
                "mysql://user:password@localhost:3306/app?ssl-mode=VERIFY_IDENTITY",
            );

            assert_eq!(ca.kind, ConnectionErrorKind::MySqlCaVerificationFailed);
            assert_eq!(
                identity.kind,
                ConnectionErrorKind::MySqlHostnameVerificationFailed
            );
        }

        #[test]
        fn from_db_operation_error_preserves_connection_lost_kind() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionLost("connection to server was lost".to_string()),
            );

            assert_eq!(info.kind, ConnectionErrorKind::ConnectionLost);
        }

        #[test]
        fn from_db_operation_error_preserves_permission_denied_kind() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::PermissionDenied(
                    "ERROR 1044 (42000): Access denied to database".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::PermissionDenied);
            assert_eq!(info.kind.hint(), "Check the connected user's privileges");
        }

        #[test]
        fn from_db_operation_error_classifies_sqlite_missing_file() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "SQLite database file not found: /tmp/missing.db".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::SqliteFileNotFound);
            assert_eq!(info.kind.summary(), "SQLite database file not found");
        }

        #[test]
        fn from_db_operation_error_classifies_missing_sqlite_cli() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::CommandNotFound {
                    command: DatabaseCli::Sqlite3,
                    details: "No such file or directory".to_string(),
                });

            assert_eq!(info.kind, ConnectionErrorKind::SqliteCliNotFound);
            assert_eq!(info.kind.summary(), "sqlite3 not found");
            assert_eq!(info.kind.hint(), "Install sqlite3 and add it to PATH");
        }

        #[rstest]
        #[case(
            DatabaseCli::MySql,
            ConnectionErrorKind::MySqlCliNotFound,
            "mysql not found"
        )]
        fn from_db_operation_error_classifies_missing_mysql_cli(
            #[case] command: DatabaseCli,
            #[case] expected_kind: ConnectionErrorKind,
            #[case] expected_summary: &str,
        ) {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::CommandNotFound {
                    command,
                    details: "No such file or directory".to_string(),
                });

            assert_eq!(info.kind, expected_kind);
            assert_eq!(info.kind.summary(), expected_summary);
        }

        #[rstest]
        #[case(
            UnsupportedOperationKind::ClientVersion,
            ConnectionErrorKind::MySqlCliVersionUnsupported
        )]
        #[case(
            UnsupportedOperationKind::ServerVersion,
            ConnectionErrorKind::MySqlServerVersionUnsupported
        )]
        #[case(
            UnsupportedOperationKind::SessionMode,
            ConnectionErrorKind::MySqlSqlModeUnsupported
        )]
        fn from_db_operation_error_classifies_mysql_requirements(
            #[case] kind: UnsupportedOperationKind,
            #[case] expected_kind: ConnectionErrorKind,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperationWithKind {
                    kind,
                    details: "provider details".to_string(),
                },
            );

            assert_eq!(info.kind.summary(), expected_kind.summary());
            assert_eq!(info.kind.hint(), expected_kind.hint());
            assert!(!info.is_retryable());
            assert_eq!(info.kind, expected_kind);
        }

        #[rstest]
        #[case(
            ConnectionFailureKind::TlsHandshake,
            ConnectionErrorKind::MySqlTlsHandshakeFailed
        )]
        #[case(
            ConnectionFailureKind::TlsCaVerification,
            ConnectionErrorKind::MySqlCaVerificationFailed
        )]
        #[case(
            ConnectionFailureKind::TlsHostnameVerification,
            ConnectionErrorKind::MySqlHostnameVerificationFailed
        )]
        #[case(
            ConnectionFailureKind::TlsClientCertificateRejected,
            ConnectionErrorKind::MySqlClientCertificateRejected
        )]
        fn from_db_operation_error_classifies_typed_mysql_tls(
            #[case] kind: ConnectionFailureKind,
            #[case] expected_kind: ConnectionErrorKind,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionFailedWithKind {
                    kind,
                    details: "tls details".to_string(),
                },
            );

            assert_eq!(info.kind, expected_kind);
            assert_eq!(info.kind.summary(), expected_kind.summary());
            assert_eq!(info.kind.hint(), expected_kind.hint());
            assert!(!info.is_retryable());
        }

        #[test]
        fn unknown_connection_error_fails_closed_without_leaking_details() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "hostname mismatch password=secret unexpected provider failure".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::Unknown);
            assert!(!info.masked_details().contains("secret"));
            assert!(!info.is_retryable());
        }

        #[rstest]
        #[case(
            "SQLite path is a directory, not a file: /tmp/dir.db",
            ConnectionErrorKind::SqlitePathIsDirectory
        )]
        #[case(
            "SQLite path is not a regular file: /tmp/pipe.db",
            ConnectionErrorKind::SqlitePathNotRegularFile
        )]
        #[case(
            "File is readable but not a SQLite database: /tmp/not-db",
            ConnectionErrorKind::SqliteNotDatabaseFile
        )]
        #[case(
            "Cannot read SQLite database file: /tmp/app.db: permission denied",
            ConnectionErrorKind::SqliteReadAccessDenied
        )]
        #[case(
            "Cannot access SQLite database file: /tmp/app.db: permission denied",
            ConnectionErrorKind::SqlitePathAccessDenied
        )]
        #[case(
            "Cannot read SQLite database file metadata: /tmp/app.db: device offline",
            ConnectionErrorKind::SqlitePathIo
        )]
        fn from_db_operation_error_classifies_sqlite_path_errors(
            #[case] details: &str,
            #[case] expected_kind: ConnectionErrorKind,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionFailed(details.to_string()),
            );

            assert_eq!(info.kind, expected_kind);
        }

        #[test]
        fn from_db_operation_error_classifies_sqlite_table_list_requirement() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperation(format!(
                    "{SQLITE_TABLE_LIST_REQUIRED_MARKER}: upgrade sqlite3"
                )),
            );

            assert_eq!(info.kind, ConnectionErrorKind::SqliteVersionTooOld);
            assert_eq!(info.kind.summary(), "SQLite 3.41.1 or later required");
        }

        #[test]
        fn from_db_operation_error_classifies_sqlite_safe_mode_requirement() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperation(format!(
                    "{SQLITE_SAFE_MODE_REQUIRED_MARKER}: sqlite3 3.41.1 or later is required for safe SQLite execution (found sqlite3 3.41.0)"
                )),
            );

            assert_eq!(info.kind, ConnectionErrorKind::SqliteVersionTooOld);
            assert_eq!(info.kind.summary(), "SQLite 3.41.1 or later required");
            assert_eq!(info.kind.hint(), "Upgrade sqlite3 to use SQLite safely");
        }
    }

    mod mask_password {
        use super::*;

        #[rstest]
        #[case("postgres://user:secret@host", "postgres://user:****@host")]
        #[case("postgresql://user:secret@host", "postgresql://user:****@host")]
        #[case("POSTGRES://user:secret@host", "POSTGRES://user:****@host")]
        #[case("PostgreSQL://user:secret@host", "PostgreSQL://user:****@host")]
        fn masks_postgres_url_scheme(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(mask_password(input), expected);
        }

        #[rstest]
        #[case("password=mysecret host=localhost", "password=**** host=localhost")]
        #[case("PASSWORD=mysecret host=localhost", "PASSWORD=**** host=localhost")]
        #[case("PGPASSWORD=secret123 psql", "PGPASSWORD=**** psql")]
        #[case("pgpassword=secret123 psql", "pgpassword=**** psql")]
        fn masks_key_value_dsn(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(mask_password(input), expected);
        }

        #[rstest]
        #[case("mysql://user:secret@host", "mysql://user:****@host")]
        #[case("MYSQL_PASSWORD=secret123 mysql", "MYSQL_PASSWORD=**** mysql")]
        #[case("MYSQL_PWD=secret123 mysql", "MYSQL_PWD=**** mysql")]
        fn masks_mysql_credentials(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(mask_password(input), expected);
        }

        #[test]
        fn passthrough_when_no_password() {
            assert_eq!(mask_password("no password here"), "no password here");
        }

        #[test]
        fn info_keeps_only_masked_details() {
            let info = ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::Unknown,
                "postgres://user:secret@host",
            );
            assert!(!info.masked_details().contains("secret"));
            assert_eq!(info.masked_details(), "postgres://user:****@host");
        }
    }
}
