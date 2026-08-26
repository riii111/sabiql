use crate::policy::password_masking::mask_password;
use crate::policy::sqlite_path::connection_error_kind;
use crate::ports::outbound::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, SQLITE_SAFE_MODE_REQUIRED_MARKER,
    SQLITE_TABLE_LIST_REQUIRED_MARKER, UnsupportedOperationKind,
};
use sabiql_domain::connection::MySqlSslMode;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionErrorKind {
    CliNotFound,
    MySqlCliNotFound,
    MySqlUnsupportedOperation(UnsupportedOperationKind),
    MySqlConnectionFailure(ConnectionFailureKind),
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
    fn presentation(self) -> (&'static str, &'static str) {
        match self {
            Self::CliNotFound => (
                "Database CLI not found",
                "Install the database CLI (e.g. psql) and add it to PATH",
            ),
            Self::MySqlCliNotFound => (
                DatabaseCli::MySql.not_found_summary(),
                DatabaseCli::MySql.not_found_hint(),
            ),
            Self::MySqlUnsupportedOperation(kind) => kind.presentation(),
            Self::MySqlConnectionFailure(kind) => kind.presentation(),
            Self::SqliteCliNotFound => (
                DatabaseCli::Sqlite3.not_found_summary(),
                DatabaseCli::Sqlite3.not_found_hint(),
            ),
            Self::HostUnreachable => ("Could not resolve host", "Check the hostname"),
            Self::AuthFailed => ("Authentication failed", "Check username and password"),
            Self::PermissionDenied => {
                ("Permission denied", "Check the connected user's privileges")
            }
            Self::DatabaseNotFound => ("Database does not exist", "Check database name"),
            Self::ConnectionLost => (
                "Connection lost during operation",
                "Reconnect and retry the operation",
            ),
            Self::Timeout => ("Connection timed out", "Check network connectivity"),
            Self::ConnectionRefused => (
                "Connection refused",
                "Check the host, port, and server availability",
            ),
            Self::SqliteVersionTooOld => (
                "SQLite 3.41.1 or later required",
                "Upgrade sqlite3 to use SQLite safely",
            ),
            Self::SqliteFileNotFound => (
                "SQLite database file not found",
                "Check the file path — sabiql does not create new database files",
            ),
            Self::SqlitePathIsDirectory => (
                "SQLite path is a directory",
                "Enter a path to a database file, not a folder",
            ),
            Self::SqlitePathNotRegularFile => (
                "SQLite path is not a regular file",
                "Enter a path to a regular database file, not a pipe or special file",
            ),
            Self::SqliteNotDatabaseFile => (
                "File is not a SQLite database",
                "Choose a readable SQLite database file, or create one with sqlite3",
            ),
            Self::SqliteReadAccessDenied => (
                "Cannot read SQLite database file",
                "Check read permissions for the database file",
            ),
            Self::SqlitePathAccessDenied => (
                "Cannot access SQLite database file",
                "Check file permissions for the database file",
            ),
            Self::SqlitePathIo => (
                "Cannot open SQLite database file",
                "Check that the database file path is valid and accessible",
            ),
            Self::Unknown => ("Connection failed", "See details for more information"),
        }
    }

    pub fn summary(self) -> &'static str {
        self.presentation().0
    }

    pub fn hint(self) -> &'static str {
        self.presentation().1
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
            DbOperationError::UnsupportedOperationWithKind { kind, .. } => {
                ConnectionErrorKind::MySqlUnsupportedOperation(*kind)
            }
            DbOperationError::UnsupportedOperation(details)
                if details.contains(SQLITE_TABLE_LIST_REQUIRED_MARKER)
                    || details.contains(SQLITE_SAFE_MODE_REQUIRED_MARKER) =>
            {
                ConnectionErrorKind::SqliteVersionTooOld
            }
            DbOperationError::ConnectionFailedWithKind { kind, .. } => match kind {
                ConnectionFailureKind::HostUnreachable => ConnectionErrorKind::HostUnreachable,
                ConnectionFailureKind::Auth => ConnectionErrorKind::AuthFailed,
                ConnectionFailureKind::DatabaseNotFound => ConnectionErrorKind::DatabaseNotFound,
                ConnectionFailureKind::ConnectionRefused => ConnectionErrorKind::ConnectionRefused,
                ConnectionFailureKind::TlsHandshake
                | ConnectionFailureKind::TlsCaVerification
                | ConnectionFailureKind::TlsHostnameVerification
                | ConnectionFailureKind::TlsClientCertificateRejected
                | ConnectionFailureKind::TlsCertificateVerification => {
                    ConnectionErrorKind::MySqlConnectionFailure(*kind)
                }
            },
            DbOperationError::SqlitePath(error) => connection_error_kind(error),
            _ => ConnectionErrorKind::Unknown,
        };
        Self::with_kind(kind, raw_details)
    }

    pub fn from_db_operation_error_with_dsn(error: &DbOperationError, dsn: &str) -> Self {
        let mut info = Self::from_db_operation_error(error);
        info.kind = mysql_ssl_mode_from_dsn(dsn)
            .and_then(|ssl_mode| match (ssl_mode, error) {
                (
                    MySqlSslMode::VerifyCa,
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::TlsCertificateVerification,
                        ..
                    },
                ) => Some(ConnectionErrorKind::MySqlConnectionFailure(
                    ConnectionFailureKind::TlsCaVerification,
                )),
                (
                    MySqlSslMode::VerifyIdentity,
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::TlsCertificateVerification,
                        ..
                    },
                ) => Some(ConnectionErrorKind::MySqlConnectionFailure(
                    ConnectionFailureKind::TlsHostnameVerification,
                )),
                _ => None,
            })
            .unwrap_or(info.kind);

        info
    }

    pub fn masked_details(&self) -> &str {
        &self.masked_details
    }

    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SqlitePathError;
    use rstest::rstest;

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
        #[case(ConnectionErrorKind::MySqlUnsupportedOperation(
            UnsupportedOperationKind::ClientVersion
        ))]
        #[case(ConnectionErrorKind::MySqlUnsupportedOperation(
            UnsupportedOperationKind::ServerVersion
        ))]
        #[case(ConnectionErrorKind::MySqlUnsupportedOperation(
            UnsupportedOperationKind::SessionMode
        ))]
        #[case(ConnectionErrorKind::MySqlConnectionFailure(ConnectionFailureKind::TlsHandshake))]
        #[case(ConnectionErrorKind::MySqlConnectionFailure(
            ConnectionFailureKind::TlsCaVerification
        ))]
        #[case(ConnectionErrorKind::MySqlConnectionFailure(
            ConnectionFailureKind::TlsHostnameVerification
        ))]
        #[case(ConnectionErrorKind::MySqlConnectionFailure(
            ConnectionFailureKind::TlsClientCertificateRejected
        ))]
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

        #[rstest]
        #[case(
            ConnectionFailureKind::HostUnreachable,
            ConnectionErrorKind::HostUnreachable,
            true
        )]
        #[case(ConnectionFailureKind::Auth, ConnectionErrorKind::AuthFailed, false)]
        #[case(
            ConnectionFailureKind::DatabaseNotFound,
            ConnectionErrorKind::DatabaseNotFound,
            false
        )]
        #[case(
            ConnectionFailureKind::ConnectionRefused,
            ConnectionErrorKind::ConnectionRefused,
            true
        )]
        fn from_db_operation_error_maps_typed_connection_failures(
            #[case] kind: ConnectionFailureKind,
            #[case] expected_kind: ConnectionErrorKind,
            #[case] expected_retryable: bool,
        ) {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionFailedWithKind {
                    kind,
                    details: "password=secret provider details".to_string(),
                },
            );

            assert_eq!(info.kind, expected_kind);
            assert!(!info.masked_details().contains("secret"));
            assert_eq!(info.is_retryable(), expected_retryable);
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

            assert_eq!(
                ca.kind,
                ConnectionErrorKind::MySqlConnectionFailure(
                    ConnectionFailureKind::TlsCaVerification
                )
            );
            assert_eq!(
                identity.kind,
                ConnectionErrorKind::MySqlConnectionFailure(
                    ConnectionFailureKind::TlsHostnameVerification
                )
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
            let info = ConnectionErrorInfo::from_db_operation_error(&DbOperationError::SqlitePath(
                SqlitePathError::FileNotFound("/tmp/missing.db".to_string()),
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
            ConnectionErrorKind::MySqlUnsupportedOperation(
                UnsupportedOperationKind::ClientVersion
            )
        )]
        #[case(
            UnsupportedOperationKind::ServerVersion,
            ConnectionErrorKind::MySqlUnsupportedOperation(
                UnsupportedOperationKind::ServerVersion
            )
        )]
        #[case(
            UnsupportedOperationKind::SessionMode,
            ConnectionErrorKind::MySqlUnsupportedOperation(UnsupportedOperationKind::SessionMode)
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
            ConnectionErrorKind::MySqlConnectionFailure(ConnectionFailureKind::TlsHandshake)
        )]
        #[case(
            ConnectionFailureKind::TlsCaVerification,
            ConnectionErrorKind::MySqlConnectionFailure(ConnectionFailureKind::TlsCaVerification)
        )]
        #[case(
            ConnectionFailureKind::TlsHostnameVerification,
            ConnectionErrorKind::MySqlConnectionFailure(
                ConnectionFailureKind::TlsHostnameVerification
            )
        )]
        #[case(
            ConnectionFailureKind::TlsClientCertificateRejected,
            ConnectionErrorKind::MySqlConnectionFailure(
                ConnectionFailureKind::TlsClientCertificateRejected
            )
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
            SqlitePathError::IsDirectory("/tmp/dir.db".to_string()),
            ConnectionErrorKind::SqlitePathIsDirectory
        )]
        #[case(
            SqlitePathError::NotRegularFile("/tmp/pipe.db".to_string()),
            ConnectionErrorKind::SqlitePathNotRegularFile
        )]
        #[case(
            SqlitePathError::NotDatabaseFile("/tmp/not-db".to_string()),
            ConnectionErrorKind::SqliteNotDatabaseFile
        )]
        #[case(
            SqlitePathError::ReadAccessDenied(
                "/tmp/app.db: permission denied".to_string(),
            ),
            ConnectionErrorKind::SqliteReadAccessDenied
        )]
        #[case(
            SqlitePathError::PathAccessDenied(
                "/tmp/app.db: permission denied".to_string(),
            ),
            ConnectionErrorKind::SqlitePathAccessDenied
        )]
        #[case(
            SqlitePathError::Io("/tmp/app.db: device offline".to_string()),
            ConnectionErrorKind::SqlitePathIo
        )]
        fn from_db_operation_error_classifies_sqlite_path_errors(
            #[case] error: SqlitePathError,
            #[case] expected_kind: ConnectionErrorKind,
        ) {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::SqlitePath(error));

            assert_eq!(info.kind, expected_kind);
        }

        #[test]
        fn generic_connection_failure_with_sqlite_prefix_stays_unknown() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "SQLite database file not found: /tmp/missing.db".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::Unknown);
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
