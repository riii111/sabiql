use crate::domain::SqlitePathError;
use crate::policy::password_masking::mask_password;
use crate::ports::outbound::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, SqliteCompatibilityKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionErrorInfo {
    summary: &'static str,
    hint: &'static str,
    retryable: bool,
    masked_details: String,
}

impl ConnectionErrorInfo {
    pub(crate) fn from_parts(
        summary: &'static str,
        hint: &'static str,
        retryable: bool,
        raw_stderr: impl Into<String>,
    ) -> Self {
        let raw_details = raw_stderr.into();
        let masked_details = mask_password(&raw_details);

        Self {
            summary,
            hint,
            retryable,
            masked_details,
        }
    }

    pub fn from_db_operation_error(error: &DbOperationError) -> Self {
        let raw_details = error.raw_details().into_owned();
        let (summary, hint, retryable) = match error {
            DbOperationError::CommandNotFound {
                command: DatabaseCli::Sqlite3,
                ..
            } => (
                DatabaseCli::Sqlite3.not_found_summary(),
                DatabaseCli::Sqlite3.not_found_hint(),
                false,
            ),
            DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                ..
            } => (
                DatabaseCli::MySql.not_found_summary(),
                DatabaseCli::MySql.not_found_hint(),
                false,
            ),
            DbOperationError::CommandNotFound { .. } => (
                "Database CLI not found",
                "Install the database CLI (e.g. psql) and add it to PATH",
                false,
            ),
            DbOperationError::ConnectionLost(_) => (
                "Connection lost during operation",
                "Reconnect and retry the operation",
                true,
            ),
            DbOperationError::Timeout(_) => {
                ("Connection timed out", "Check network connectivity", true)
            }
            DbOperationError::PermissionDenied(_) => (
                "Permission denied",
                "Check the connected user's privileges",
                false,
            ),
            DbOperationError::UnsupportedOperationWithKind { kind, .. } => {
                let (summary, hint) = kind.presentation();
                (summary, hint, false)
            }
            DbOperationError::UnsupportedOperationWithSqliteKind {
                kind: SqliteCompatibilityKind::SafeMode | SqliteCompatibilityKind::TableList,
                ..
            } => (
                "SQLite 3.41.1 or later required",
                "Upgrade sqlite3 to use SQLite safely",
                false,
            ),
            DbOperationError::ConnectionFailedWithKind { kind, .. } => match kind {
                ConnectionFailureKind::HostUnreachable => {
                    ("Could not resolve host", "Check the hostname", true)
                }
                ConnectionFailureKind::Auth => (
                    "Authentication failed",
                    "Check username and password",
                    false,
                ),
                ConnectionFailureKind::DatabaseNotFound => {
                    ("Database does not exist", "Check database name", false)
                }
                ConnectionFailureKind::ConnectionRefused => (
                    "Connection refused",
                    "Check the host, port, and server availability",
                    true,
                ),
                ConnectionFailureKind::TlsHandshake
                | ConnectionFailureKind::TlsCaVerification
                | ConnectionFailureKind::TlsHostnameVerification
                | ConnectionFailureKind::TlsClientCertificateRejected
                | ConnectionFailureKind::TlsCertificateVerification => {
                    let (summary, hint) = kind.presentation();
                    (summary, hint, false)
                }
            },
            DbOperationError::SqlitePath(error) => sqlite_path_presentation(error),
            _ => (
                "Connection failed",
                "See details for more information",
                false,
            ),
        };
        Self::from_parts(summary, hint, retryable, raw_details)
    }

    pub fn summary(&self) -> &'static str {
        self.summary
    }

    pub fn hint(&self) -> &'static str {
        self.hint
    }

    pub fn masked_details(&self) -> &str {
        &self.masked_details
    }

    pub const fn is_retryable(&self) -> bool {
        self.retryable
    }
}

fn sqlite_path_presentation(error: &SqlitePathError) -> (&'static str, &'static str, bool) {
    match error {
        SqlitePathError::FileNotFound(_) => (
            "SQLite database file not found",
            "Check the file path — sabiql does not create new database files",
            false,
        ),
        SqlitePathError::IsDirectory(_) => (
            "SQLite path is a directory",
            "Enter a path to a database file, not a folder",
            false,
        ),
        SqlitePathError::NotRegularFile(_) => (
            "SQLite path is not a regular file",
            "Enter a path to a regular database file, not a pipe or special file",
            false,
        ),
        SqlitePathError::NotDatabaseFile(_) => (
            "File is not a SQLite database",
            "Choose a readable SQLite database file, or create one with sqlite3",
            false,
        ),
        SqlitePathError::ReadAccessDenied(_) => (
            "Cannot read SQLite database file",
            "Check read permissions for the database file",
            false,
        ),
        SqlitePathError::PathAccessDenied(_) => (
            "Cannot access SQLite database file",
            "Check file permissions for the database file",
            false,
        ),
        SqlitePathError::Io(_) => (
            "Cannot open SQLite database file",
            "Check that the database file path is valid and accessible",
            false,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SqlitePathError;
    use crate::ports::outbound::UnsupportedOperationKind;
    use rstest::rstest;

    mod error_info {
        use super::*;

        #[test]
        fn from_parts_uses_provided_presentation() {
            let info = ConnectionErrorInfo::from_parts(
                "Connection timed out",
                "Check network connectivity",
                true,
                "error",
            );
            assert_eq!(info.summary(), "Connection timed out");
            assert_eq!(info.hint(), "Check network connectivity");
            assert!(info.is_retryable());
        }

        #[rstest]
        #[case(
            ConnectionFailureKind::HostUnreachable,
            "Could not resolve host",
            "Check the hostname",
            true
        )]
        #[case(
            ConnectionFailureKind::Auth,
            "Authentication failed",
            "Check username and password",
            false
        )]
        #[case(
            ConnectionFailureKind::DatabaseNotFound,
            "Database does not exist",
            "Check database name",
            false
        )]
        #[case(
            ConnectionFailureKind::ConnectionRefused,
            "Connection refused",
            "Check the host, port, and server availability",
            true
        )]
        fn from_db_operation_error_maps_typed_connection_failures(
            #[case] kind: ConnectionFailureKind,
            #[case] expected_summary: &str,
            #[case] expected_hint: &str,
            #[case] expected_retryable: bool,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionFailedWithKind {
                    kind,
                    details: "password=secret provider details".to_string(),
                },
            );

            assert_eq!(info.summary(), expected_summary);
            assert_eq!(info.hint(), expected_hint);
            assert!(!info.masked_details().contains("secret"));
            assert_eq!(info.is_retryable(), expected_retryable);
        }

        #[test]
        fn from_db_operation_error_preserves_connection_lost_presentation() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionLost("connection to server was lost".to_string()),
            );

            assert_eq!(info.summary(), "Connection lost during operation");
            assert_eq!(info.hint(), "Reconnect and retry the operation");
            assert!(info.is_retryable());
        }

        #[test]
        fn from_db_operation_error_preserves_permission_denied_presentation() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::PermissionDenied(
                    "ERROR 1044 (42000): Access denied to database".to_string(),
                ));

            assert_eq!(info.summary(), "Permission denied");
            assert_eq!(info.hint(), "Check the connected user's privileges");
            assert!(!info.is_retryable());
        }

        #[test]
        fn from_db_operation_error_classifies_sqlite_missing_file() {
            let info = ConnectionErrorInfo::from_db_operation_error(&DbOperationError::SqlitePath(
                SqlitePathError::FileNotFound("/tmp/missing.db".to_string()),
            ));

            assert_eq!(info.summary(), "SQLite database file not found");
            assert_eq!(
                info.hint(),
                "Check the file path — sabiql does not create new database files"
            );
        }

        #[test]
        fn from_db_operation_error_classifies_missing_sqlite_cli() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::CommandNotFound {
                    command: DatabaseCli::Sqlite3,
                    details: "No such file or directory".to_string(),
                });

            assert_eq!(info.summary(), "sqlite3 not found");
            assert_eq!(info.hint(), "Install sqlite3 and add it to PATH");
        }

        #[rstest]
        #[case(DatabaseCli::MySql, "mysql not found")]
        fn from_db_operation_error_classifies_missing_mysql_cli(
            #[case] command: DatabaseCli,
            #[case] expected_summary: &str,
        ) {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::CommandNotFound {
                    command,
                    details: "No such file or directory".to_string(),
                });

            assert_eq!(info.summary(), expected_summary);
        }

        #[rstest]
        #[case(
            UnsupportedOperationKind::ClientVersion,
            "Unsupported MySQL CLI version",
            "Install the Oracle MySQL 8.4 client"
        )]
        #[case(
            UnsupportedOperationKind::ServerVersion,
            "Unsupported MySQL server version",
            "Connect to an Oracle MySQL 8.4 server"
        )]
        #[case(
            UnsupportedOperationKind::SessionMode,
            "Unsupported MySQL sql_mode",
            "Disable NO_BACKSLASH_ESCAPES and ANSI_QUOTES for this connection"
        )]
        fn from_db_operation_error_classifies_mysql_requirements(
            #[case] kind: UnsupportedOperationKind,
            #[case] expected_summary: &str,
            #[case] expected_hint: &str,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperationWithKind {
                    kind,
                    details: "provider details".to_string(),
                },
            );

            assert_eq!(info.summary(), expected_summary);
            assert_eq!(info.hint(), expected_hint);
            assert!(!info.is_retryable());
        }

        #[rstest]
        #[case(
            ConnectionFailureKind::TlsHandshake,
            "MySQL TLS handshake failed",
            "Check that the server and client support the selected TLS settings"
        )]
        #[case(
            ConnectionFailureKind::TlsCertificateVerification,
            "MySQL TLS handshake failed",
            "Check that the server and client support the selected TLS settings"
        )]
        #[case(
            ConnectionFailureKind::TlsCaVerification,
            "MySQL server certificate could not be verified",
            "Check the CA certificate path and server certificate"
        )]
        #[case(
            ConnectionFailureKind::TlsHostnameVerification,
            "MySQL server hostname could not be verified",
            "Use the hostname covered by the server certificate"
        )]
        #[case(
            ConnectionFailureKind::TlsClientCertificateRejected,
            "MySQL client certificate was rejected",
            "Check the client certificate, key, and server account requirements"
        )]
        fn from_db_operation_error_classifies_typed_mysql_tls(
            #[case] kind: ConnectionFailureKind,
            #[case] expected_summary: &str,
            #[case] expected_hint: &str,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionFailedWithKind {
                    kind,
                    details: "tls details".to_string(),
                },
            );

            assert_eq!(info.summary(), expected_summary);
            assert_eq!(info.hint(), expected_hint);
            assert!(!info.is_retryable());
        }

        #[test]
        fn unknown_connection_error_fails_closed_without_leaking_details() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "hostname mismatch password=secret unexpected provider failure".to_string(),
                ));

            assert_eq!(info.summary(), "Connection failed");
            assert_eq!(info.hint(), "See details for more information");
            assert!(!info.masked_details().contains("secret"));
            assert!(!info.is_retryable());
        }

        #[rstest]
        #[case(
            SqlitePathError::IsDirectory("/tmp/dir.db".to_string()),
            "SQLite path is a directory"
        )]
        #[case(
            SqlitePathError::NotRegularFile("/tmp/pipe.db".to_string()),
            "SQLite path is not a regular file"
        )]
        #[case(
            SqlitePathError::NotDatabaseFile("/tmp/not-db".to_string()),
            "File is not a SQLite database"
        )]
        #[case(
            SqlitePathError::ReadAccessDenied(
                "/tmp/app.db: permission denied".to_string(),
            ),
            "Cannot read SQLite database file"
        )]
        #[case(
            SqlitePathError::PathAccessDenied(
                "/tmp/app.db: permission denied".to_string(),
            ),
            "Cannot access SQLite database file"
        )]
        #[case(
            SqlitePathError::Io("/tmp/app.db: device offline".to_string()),
            "Cannot open SQLite database file"
        )]
        fn from_db_operation_error_classifies_sqlite_path_errors(
            #[case] error: SqlitePathError,
            #[case] expected_summary: &str,
        ) {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::SqlitePath(error));

            assert_eq!(info.summary(), expected_summary);
        }

        #[test]
        fn generic_connection_failure_with_sqlite_prefix_stays_unknown() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "SQLite database file not found: /tmp/missing.db".to_string(),
                ));

            assert_eq!(info.summary(), "Connection failed");
        }

        #[test]
        fn from_db_operation_error_classifies_typed_sqlite_table_list_requirement() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperationWithSqliteKind {
                    kind: SqliteCompatibilityKind::TableList,
                    details: "upgrade sqlite3 to version 3.41.1 or later".to_string(),
                },
            );

            assert_eq!(info.summary(), "SQLite 3.41.1 or later required");
            assert_eq!(info.hint(), "Upgrade sqlite3 to use SQLite safely");
            assert_eq!(
                info.masked_details(),
                "upgrade sqlite3 to version 3.41.1 or later"
            );
        }

        #[test]
        fn from_db_operation_error_classifies_typed_sqlite_safe_mode_requirement() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperationWithSqliteKind {
                    kind: SqliteCompatibilityKind::SafeMode,
                    details: "sqlite3 3.41.1 or later is required for safe SQLite execution (found sqlite3 3.41.0)".to_string(),
                },
            );

            assert_eq!(info.summary(), "SQLite 3.41.1 or later required");
            assert_eq!(info.hint(), "Upgrade sqlite3 to use SQLite safely");
            assert_eq!(
                info.masked_details(),
                "sqlite3 3.41.1 or later is required for safe SQLite execution (found sqlite3 3.41.0)"
            );
        }

        #[test]
        fn marker_like_unsupported_operation_stays_unknown() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::UnsupportedOperation(
                    "SQLITE_SAFE_MODE_REQUIRED: sqlite3 3.41.1".to_string(),
                ),
            );

            assert_eq!(info.summary(), "Connection failed");
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
            let info = ConnectionErrorInfo::from_parts(
                "Connection failed",
                "See details for more information",
                false,
                "postgres://user:secret@host",
            );
            assert!(!info.masked_details().contains("secret"));
            assert_eq!(info.masked_details(), "postgres://user:****@host");
        }
    }
}
