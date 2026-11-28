use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use crate::domain::{RefreshScope, SqlitePathError};
use crate::policy::password_masking::mask_password;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseCli {
    Psql,
    Sqlite3,
    MySql,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedOperationKind {
    ClientVersion,
    ServerProduct,
    SessionMode,
}

impl UnsupportedOperationKind {
    pub(crate) const fn presentation(self) -> (&'static str, &'static str) {
        match self {
            Self::ClientVersion => (
                "Unsupported MySQL CLI version",
                "Install the Oracle MySQL 8.4 client; the MySQL CLI is required",
            ),
            Self::ServerProduct => (
                "Unsupported MySQL server product",
                "Connect to an Oracle MySQL server; 8.4 is continuously validated, other versions are not fully guaranteed",
            ),
            Self::SessionMode => (
                "Unsupported MySQL sql_mode",
                "Disable NO_BACKSLASH_ESCAPES and ANSI_QUOTES for this connection",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteCompatibilityKind {
    SafeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionFailureKind {
    HostUnreachable,
    Auth,
    DatabaseNotFound,
    ConnectionRefused,
    TlsHandshake,
    TlsCaVerification,
    TlsHostnameVerification,
    TlsClientCertificateRejected,
    TlsCertificateVerification,
}

impl ConnectionFailureKind {
    pub(crate) const fn presentation(self) -> (&'static str, &'static str) {
        match self {
            Self::HostUnreachable
            | Self::Auth
            | Self::DatabaseNotFound
            | Self::ConnectionRefused => (
                "Connection failed",
                "Check the connection settings and database availability",
            ),
            Self::TlsHandshake | Self::TlsCertificateVerification => (
                "MySQL TLS handshake failed",
                "Check that the server and client support the selected TLS settings",
            ),
            Self::TlsCaVerification => (
                "MySQL server certificate could not be verified",
                "Check the CA certificate path and server certificate",
            ),
            Self::TlsHostnameVerification => (
                "MySQL server hostname could not be verified",
                "Use the hostname covered by the server certificate",
            ),
            Self::TlsClientCertificateRejected => (
                "MySQL client certificate was rejected",
                "Check the client certificate, key, and server account requirements",
            ),
        }
    }
}

#[derive(Clone)]
pub enum DbOperationError {
    ConnectionFailed(String),
    SqlitePath(SqlitePathError),
    ConnectionLost(String),
    PermissionDenied(String),
    LockTimeout(String),
    ObjectMissing(String),
    QueryFailed(String),
    ExportIo(Arc<std::io::Error>),
    PreviewSizeExceeded(String),
    QueryFailedAfterChange {
        source: Arc<Self>,
        refresh_scope: RefreshScope,
    },
    UnsupportedOperation(String),
    UnsupportedOperationWithKind {
        kind: UnsupportedOperationKind,
        details: String,
    },
    UnsupportedOperationWithSqliteKind {
        kind: SqliteCompatibilityKind,
        details: String,
    },
    ConnectionFailedWithKind {
        kind: ConnectionFailureKind,
        details: String,
    },
    CommandNotFound {
        command: DatabaseCli,
        details: String,
    },
    Timeout(String),
    Canceled(String),
}

impl DbOperationError {
    pub fn post_change_refresh_scope(&self) -> Option<RefreshScope> {
        match self {
            Self::QueryFailedAfterChange { refresh_scope, .. } => Some(*refresh_scope),
            _ => None,
        }
    }

    pub(crate) fn connection_presentation(&self) -> (&'static str, &'static str, bool) {
        self.presentation(true)
    }

    fn presentation(&self, connecting: bool) -> (&'static str, &'static str, bool) {
        match self {
            Self::CommandNotFound { command, .. } => match command {
                DatabaseCli::Psql if connecting => (
                    "Database CLI not found",
                    "Install the database CLI (e.g. psql) and add it to PATH",
                    false,
                ),
                DatabaseCli::Psql => (
                    "Database CLI not found",
                    "Install the database client and add it to PATH",
                    false,
                ),
                DatabaseCli::Sqlite3 => (
                    "sqlite3 not found",
                    "Install sqlite3 and add it to PATH",
                    false,
                ),
                DatabaseCli::MySql => (
                    "mysql not found",
                    "Install the Oracle MySQL 8.4 client and add it to PATH",
                    false,
                ),
            },
            Self::ConnectionLost(_) => (
                "Connection lost during operation",
                "Reconnect and retry the operation",
                true,
            ),
            Self::Timeout(_) if connecting => {
                ("Connection timed out", "Check network connectivity", true)
            }
            Self::Timeout(_) => (
                "Operation timed out",
                "Retry the operation or increase the timeout",
                true,
            ),
            Self::PermissionDenied(_) => (
                "Permission denied",
                "Check the connected user's privileges",
                false,
            ),
            Self::UnsupportedOperationWithKind { kind, .. } => {
                let (summary, hint) = kind.presentation();
                (summary, hint, false)
            }
            Self::UnsupportedOperationWithSqliteKind {
                kind: SqliteCompatibilityKind::SafeMode,
                ..
            } if connecting => (
                "SQLite 3.41.1 or later required",
                "Upgrade sqlite3 to use SQLite safely",
                false,
            ),
            Self::ConnectionFailedWithKind { kind, .. } => match kind {
                ConnectionFailureKind::HostUnreachable if connecting => {
                    ("Could not resolve host", "Check the hostname", true)
                }
                ConnectionFailureKind::Auth if connecting => (
                    "Authentication failed",
                    "Check username and password",
                    false,
                ),
                ConnectionFailureKind::DatabaseNotFound if connecting => {
                    ("Database does not exist", "Check database name", false)
                }
                ConnectionFailureKind::ConnectionRefused if connecting => (
                    "Connection refused",
                    "Check the host, port, and server availability",
                    true,
                ),
                kind => {
                    let (summary, hint) = kind.presentation();
                    (summary, hint, false)
                }
            },
            Self::SqlitePath(error) if connecting => sqlite_path_presentation(error),
            _ if connecting => (
                "Connection failed",
                "See details for more information",
                false,
            ),
            Self::ConnectionFailed(_) | Self::SqlitePath(_) => (
                "Connection failed",
                "Check the connection settings and database availability",
                false,
            ),
            Self::LockTimeout(_) => (
                "Operation blocked by lock or timeout",
                "Retry; if it persists, check for blocking transactions or timeout settings",
                true,
            ),
            Self::ObjectMissing(_) => (
                "Database object not found",
                "Check the table, column, or connected database",
                false,
            ),
            Self::QueryFailed(_) => (
                "Query failed",
                "Review the database error details and SQL",
                false,
            ),
            Self::ExportIo(_) => (
                "CSV export failed",
                "Check the export folder and available disk space",
                false,
            ),
            Self::PreviewSizeExceeded(_) => (
                "Preview exceeded its byte budget",
                "Reduce the preview value size and retry",
                false,
            ),
            Self::QueryFailedAfterChange { source, .. } => {
                let (summary, hint, _) = source.presentation(false);
                (summary, hint, false)
            }
            Self::UnsupportedOperation(_) | Self::UnsupportedOperationWithSqliteKind { .. } => (
                "Unsupported operation",
                "Use a supported operation for this database",
                false,
            ),
            Self::Canceled(_) => ("Operation canceled", "Retry the operation if needed", false),
        }
    }

    pub fn summary(&self) -> &'static str {
        self.presentation(false).0
    }

    pub fn hint(&self) -> &'static str {
        self.presentation(false).1
    }

    pub fn masked_details(&self) -> String {
        mask_password(self.raw_details().as_ref())
    }

    pub fn user_message(&self) -> String {
        let (summary, hint, _) = self.presentation(false);
        let details = self.masked_details();

        let message = match (details.trim().is_empty(), hint.is_empty()) {
            (true, true) => summary.to_string(),
            (true, false) => format!("{summary}. {hint}."),
            (false, true) => format!("{summary}: {details}"),
            (false, false) => format!("{summary}: {details}. {hint}."),
        };

        if matches!(self, Self::QueryFailedAfterChange { .. }) {
            format!(
                "{message} Some changes may have been committed; refresh the database state before retrying."
            )
        } else {
            message
        }
    }

    pub fn result_message(&self) -> String {
        let (summary, hint, _) = self.presentation(false);
        let details = self.masked_details();

        let message = match (details.trim().is_empty(), hint.is_empty()) {
            (true, true) => summary.to_string(),
            (true, false) => format!("{summary}. {hint}."),
            (false, true) => format!("{summary}\n\nDetails:\n{details}"),
            (false, false) => format!("{summary}. {hint}.\n\nDetails:\n{details}"),
        };

        if matches!(self, Self::QueryFailedAfterChange { .. }) {
            format!(
                "{message}\n\nSome changes may have been committed; refresh the database state before retrying."
            )
        } else {
            message
        }
    }

    fn raw_details(&self) -> Cow<'_, str> {
        match self {
            Self::ConnectionFailed(details)
            | Self::ConnectionLost(details)
            | Self::PermissionDenied(details)
            | Self::LockTimeout(details)
            | Self::ObjectMissing(details)
            | Self::QueryFailed(details)
            | Self::PreviewSizeExceeded(details)
            | Self::UnsupportedOperation(details)
            | Self::UnsupportedOperationWithKind { details, .. }
            | Self::UnsupportedOperationWithSqliteKind { details, .. }
            | Self::ConnectionFailedWithKind { details, .. }
            | Self::Timeout(details)
            | Self::Canceled(details)
            | Self::CommandNotFound { details, .. } => Cow::Borrowed(details.as_str()),
            Self::SqlitePath(error) => Cow::Owned(error.to_string()),
            Self::ExportIo(error) => Cow::Owned(error.to_string()),
            Self::QueryFailedAfterChange { source, .. } => source.raw_details(),
        }
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

// Keep Display summary-only to avoid leaking raw command output.
impl fmt::Display for DbOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueryFailedAfterChange { .. } => f.write_str("Query failed after a change"),
            _ => f.write_str(self.summary()),
        }
    }
}

impl std::error::Error for DbOperationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SqlitePath(error) => Some(error),
            Self::ExportIo(error) => Some(error),
            Self::QueryFailedAfterChange { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl fmt::Debug for DbOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("DbOperationError");
        debug.field("kind", &self.summary());

        let details = self.masked_details();
        if !details.trim().is_empty() {
            debug.field("details", &details);
        }

        debug.finish()
    }
}

impl From<serde_json::Error> for DbOperationError {
    fn from(e: serde_json::Error) -> Self {
        Self::QueryFailed(e.to_string())
    }
}

impl From<csv::Error> for DbOperationError {
    fn from(e: csv::Error) -> Self {
        Self::QueryFailed(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod post_change_refresh_scope {
        use super::*;

        #[test]
        fn wrapped_none_is_distinct_from_an_unwrapped_error() {
            let source = DbOperationError::QueryFailed("failed".to_string());
            let error = DbOperationError::QueryFailedAfterChange {
                source: Arc::new(source.clone()),
                refresh_scope: RefreshScope::None,
            };

            assert_eq!(source.post_change_refresh_scope(), None);
            assert_eq!(error.post_change_refresh_scope(), Some(RefreshScope::None));
        }
    }

    mod summaries_and_hints {
        use super::*;

        #[rstest]
        #[case(ConnectionFailureKind::HostUnreachable)]
        #[case(ConnectionFailureKind::Auth)]
        #[case(ConnectionFailureKind::DatabaseNotFound)]
        #[case(ConnectionFailureKind::ConnectionRefused)]
        fn typed_connection_failures_keep_generic_operation_presentation(
            #[case] kind: ConnectionFailureKind,
        ) {
            let error = DbOperationError::ConnectionFailedWithKind {
                kind,
                details: "provider details".to_string(),
            };

            assert_eq!(error.summary(), "Connection failed");
            assert_eq!(
                error.hint(),
                "Check the connection settings and database availability"
            );
        }
    }

    mod masking {
        use super::*;

        #[rstest]
        #[case(
            DbOperationError::PermissionDenied("postgres://user:secret@host".to_string()),
            "postgres://user:****@host"
        )]
        #[case(
            DbOperationError::QueryFailed("password=mysecret host=localhost".to_string()),
            "password=**** host=localhost"
        )]
        #[case(
            DbOperationError::ConnectionFailed("PGPASSWORD=secret123 psql".to_string()),
            "PGPASSWORD=**** psql"
        )]
        #[case(
            DbOperationError::ConnectionFailed("pgpassword=secret123 psql".to_string()),
            "pgpassword=**** psql"
        )]
        #[case(
            DbOperationError::ConnectionFailed("sslpassword=mysecret host=localhost".to_string()),
            "sslpassword=**** host=localhost"
        )]
        #[case(
            DbOperationError::ConnectionFailed("postgres://user:p@ss@host".to_string()),
            "postgres://user:****@host"
        )]
        fn hides_passwords(#[case] error: DbOperationError, #[case] expected: &str) {
            assert_eq!(error.masked_details(), expected);
        }
    }

    mod user_messages {
        use super::*;
        use std::error::Error;

        #[test]
        fn sqlite_cli_not_found_has_sqlite_specific_guidance() {
            let error = DbOperationError::CommandNotFound {
                command: DatabaseCli::Sqlite3,
                details: "No such file or directory".to_string(),
            };

            assert_eq!(error.summary(), "sqlite3 not found");
            assert_eq!(error.hint(), "Install sqlite3 and add it to PATH");
        }

        #[test]
        fn sqlite_path_preserves_source_and_masks_details() {
            let error =
                DbOperationError::SqlitePath(SqlitePathError::Io("password=secret".to_string()));

            assert_eq!(
                error.masked_details(),
                "Cannot read SQLite database file metadata: password=****"
            );
            assert_eq!(
                error
                    .source()
                    .expect("SQLite path error source")
                    .to_string(),
                "Cannot read SQLite database file metadata: password=secret"
            );
            assert!(!error.user_message().contains("secret"));
        }

        #[test]
        fn mysql_cli_not_found_has_oracle_mysql_guidance() {
            let error = DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                details: "mysql: command not found".to_string(),
            };

            assert_eq!(error.summary(), "mysql not found");
            assert_eq!(
                error.hint(),
                "Install the Oracle MySQL 8.4 client and add it to PATH"
            );
            assert_eq!(
                error.user_message(),
                "mysql not found: mysql: command not found. Install the Oracle MySQL 8.4 client and add it to PATH."
            );
        }

        #[test]
        fn actionable_message_uses_summary_and_hint() {
            let error = DbOperationError::PermissionDenied("permission denied".to_string());

            assert_eq!(
                error.user_message(),
                "Permission denied: permission denied. Check the connected user's privileges."
            );
        }

        #[test]
        fn generic_query_failed_uses_consistent_format() {
            let error = DbOperationError::QueryFailed("syntax error at or near SELECT".to_string());

            assert_eq!(
                error.user_message(),
                "Query failed: syntax error at or near SELECT. Review the database error details and SQL."
            );
        }

        #[test]
        fn export_io_uses_export_guidance_and_preserves_source() {
            let error = DbOperationError::ExportIo(Arc::new(std::io::Error::other(
                "password=mysecret host=localhost",
            )));

            assert_eq!(error.summary(), "CSV export failed");
            assert_eq!(
                error.hint(),
                "Check the export folder and available disk space"
            );
            assert_eq!(error.masked_details(), "password=**** host=localhost");
            assert!(std::error::Error::source(&error).is_some());
            assert_eq!(
                error.user_message(),
                "CSV export failed: password=**** host=localhost. Check the export folder and available disk space."
            );
            assert!(!error.user_message().contains("mysecret"));
            assert!(!format!("{error:?}").contains("mysecret"));
        }

        #[test]
        fn change_failure_warns_about_possible_commits() {
            let error = DbOperationError::QueryFailedAfterChange {
                source: Arc::new(DbOperationError::QueryFailed("syntax error".to_string())),
                refresh_scope: RefreshScope::Metadata,
            };

            assert_eq!(error.summary(), "Query failed");
            assert_eq!(error.hint(), "Review the database error details and SQL");
            assert_eq!(error.masked_details(), "syntax error");
            assert!(
                error
                    .user_message()
                    .contains("Some changes may have been committed")
            );
        }

        #[rstest]
        #[case(DbOperationError::PermissionDenied("permission denied".to_string()))]
        #[case(DbOperationError::LockTimeout("lock wait timeout".to_string()))]
        fn change_failure_preserves_classification(#[case] source: DbOperationError) {
            let expected_summary = source.summary();
            let expected_hint = source.hint();
            let expected_details = source.masked_details();
            let error = DbOperationError::QueryFailedAfterChange {
                source: Arc::new(source),
                refresh_scope: RefreshScope::Data,
            };

            assert_eq!(error.summary(), expected_summary);
            assert_eq!(error.hint(), expected_hint);
            assert_eq!(error.masked_details(), expected_details);
            assert!(error.user_message().contains(expected_summary));
            assert!(error.user_message().contains(expected_hint));
        }

        #[test]
        fn change_failure_masks_nested_source_details() {
            let error = DbOperationError::QueryFailedAfterChange {
                source: Arc::new(DbOperationError::PermissionDenied(
                    "password=secret".to_string(),
                )),
                refresh_scope: RefreshScope::Data,
            };

            assert_eq!(error.masked_details(), "password=****");
            assert!(!error.user_message().contains("secret"));
            assert!(!format!("{error:?}").contains("secret"));
        }

        #[test]
        fn result_message_keeps_details_for_actionable_errors() {
            let error = DbOperationError::QueryFailed(
                "ERROR: duplicate key value violates unique constraint".to_string(),
            );

            assert!(error.result_message().contains("Query failed."));
            assert!(error.result_message().contains("Details:"));
            assert_eq!(
                error
                    .result_message()
                    .matches("ERROR: duplicate key value violates unique constraint")
                    .count(),
                1
            );
        }

        #[test]
        fn debug_uses_masked_details() {
            let error =
                DbOperationError::ConnectionFailed("postgres://user:secret@host".to_string());

            let debug = format!("{error:?}");

            assert!(debug.contains("****"));
            assert!(!debug.contains("secret"));
        }
    }
}
