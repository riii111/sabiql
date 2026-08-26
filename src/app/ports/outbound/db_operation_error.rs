use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use crate::domain::{RefreshScope, SqlitePathError};
use crate::policy::password_masking::mask_password;

pub const MYSQL_CONNECT_TIMEOUT_ERRNOS: &[&str] = &["(60)", "(110)", "(10060)"];

pub fn mysql_server_error_code(lowercase_details: &str) -> Option<u32> {
    let start = lowercase_details.find("error ")? + "error ".len();
    let digits = &lowercase_details[start..];
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

pub fn is_mysql_connect_timeout_message(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    lowercase.contains("can't connect to mysql server")
        && MYSQL_CONNECT_TIMEOUT_ERRNOS
            .iter()
            .any(|errno| lowercase.contains(errno))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseCli {
    Psql,
    Sqlite3,
    MySql,
}

impl DatabaseCli {
    pub const fn not_found_summary(self) -> &'static str {
        match self {
            Self::Psql => "Database CLI not found",
            Self::Sqlite3 => "sqlite3 not found",
            Self::MySql => "mysql not found",
        }
    }

    pub const fn not_found_hint(self) -> &'static str {
        match self {
            Self::Psql => "Install the database client and add it to PATH",
            Self::Sqlite3 => "Install sqlite3 and add it to PATH",
            Self::MySql => "Install the Oracle MySQL 8.4 client and add it to PATH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedOperationKind {
    ClientVersion,
    ServerVersion,
    SessionMode,
}

impl UnsupportedOperationKind {
    pub(crate) const fn presentation(self) -> (&'static str, &'static str) {
        match self {
            Self::ClientVersion => (
                "Unsupported MySQL CLI version",
                "Install the Oracle MySQL 8.4 client",
            ),
            Self::ServerVersion => (
                "Unsupported MySQL server version",
                "Connect to an Oracle MySQL 8.4 server",
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
    TableList,
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

#[derive(Clone, thiserror::Error)]
// Keep Display summary-only to avoid leaking raw command output.
pub enum DbOperationError {
    #[error("Connection failed")]
    ConnectionFailed(String),
    #[error("Connection failed")]
    SqlitePath(#[source] SqlitePathError),
    #[error("Connection lost")]
    ConnectionLost(String),
    #[error("Permission denied")]
    PermissionDenied(String),
    #[error("Foreign key constraint violated")]
    ForeignKeyViolation(String),
    #[error("Unique constraint violated")]
    UniqueViolation(String),
    #[error("Operation blocked by lock or timeout")]
    LockTimeout(String),
    #[error("Database object not found")]
    ObjectMissing(String),
    #[error("Query failed")]
    QueryFailed(String),
    #[error("CSV export failed")]
    ExportIo(#[source] ExportIoSource),
    #[error("Preview exceeded its byte budget")]
    PreviewSizeExceeded(String),
    #[error("Query failed after a change")]
    QueryFailedAfterChange {
        #[source]
        source: Arc<Self>,
        refresh_scope: RefreshScope,
    },
    #[error("Unsupported operation")]
    UnsupportedOperation(String),
    #[error("Unsupported operation")]
    UnsupportedOperationWithKind {
        kind: UnsupportedOperationKind,
        details: String,
    },
    #[error("Unsupported operation")]
    UnsupportedOperationWithSqliteKind {
        kind: SqliteCompatibilityKind,
        details: String,
    },
    #[error("Connection failed")]
    ConnectionFailedWithKind {
        kind: ConnectionFailureKind,
        details: String,
    },
    #[error("Metadata parse failed")]
    MetadataParseFailed(String),
    #[error("Invalid JSON")]
    InvalidJson(#[source] Arc<serde_json::Error>),
    #[error("Empty response")]
    EmptyResponse(String),
    #[error("CSV parse error")]
    CsvParse(#[source] Arc<csv::Error>),
    #[error("Command tag parse failed")]
    CommandTagParseFailed(String),
    #[error("Command not found")]
    CommandNotFound {
        command: DatabaseCli,
        details: String,
    },
    #[error("Operation timed out")]
    Timeout(String),
    #[error("Operation canceled")]
    Canceled(String),
}

#[derive(Clone)]
pub struct ExportIoSource(Arc<std::io::Error>);

impl ExportIoSource {
    pub fn new(error: std::io::Error) -> Self {
        Self(Arc::new(error))
    }
}

impl std::ops::Deref for ExportIoSource {
    type Target = std::io::Error;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl fmt::Display for ExportIoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl DbOperationError {
    pub fn post_change_refresh_scope(&self) -> Option<RefreshScope> {
        match self {
            Self::QueryFailedAfterChange { refresh_scope, .. } => Some(*refresh_scope),
            _ => None,
        }
    }

    fn presentation(&self) -> (&'static str, &'static str) {
        match self {
            Self::ConnectionFailed(_) | Self::SqlitePath(_) => (
                "Connection failed",
                "Check the connection settings and database availability",
            ),
            Self::ConnectionLost(_) => (
                "Connection lost during operation",
                "Reconnect and retry the operation",
            ),
            Self::PermissionDenied(_) => {
                ("Permission denied", "Check the connected user's privileges")
            }
            Self::ForeignKeyViolation(_) => (
                "Foreign key constraint violation",
                "Check referenced rows before retrying the write operation",
            ),
            Self::UniqueViolation(_) => (
                "Unique constraint violation",
                "Check for duplicate values before retrying",
            ),
            Self::LockTimeout(_) => (
                "Operation blocked by lock or timeout",
                "Retry; if it persists, check for blocking transactions or timeout settings",
            ),
            Self::ObjectMissing(_) => (
                "Database object not found",
                "Check the table, column, or connected database",
            ),
            Self::QueryFailed(_) => ("Query failed", "Review the database error details and SQL"),
            Self::ExportIo(_) => (
                "CSV export failed",
                "Check the export folder and available disk space",
            ),
            Self::PreviewSizeExceeded(_) => (
                "Preview exceeded its byte budget",
                "Reduce the preview value size and retry",
            ),
            Self::QueryFailedAfterChange { source, .. } => source.presentation(),
            Self::UnsupportedOperation(_) | Self::UnsupportedOperationWithSqliteKind { .. } => (
                "Unsupported operation",
                "Use a supported operation for this database",
            ),
            Self::UnsupportedOperationWithKind { kind, .. } => kind.presentation(),
            Self::ConnectionFailedWithKind { kind, .. } => kind.presentation(),
            Self::MetadataParseFailed(_) => (
                "Failed to parse database metadata output",
                "Check whether the metadata output format changed unexpectedly",
            ),
            Self::InvalidJson(_) => (
                "Failed to parse database JSON output",
                "Check whether the adapter query output shape changed",
            ),
            Self::EmptyResponse(_) => (
                "Database returned an empty response",
                "Retry the operation and inspect the command output",
            ),
            Self::CsvParse(_) => (
                "Failed to parse database CSV output",
                "Check whether the adapter returned malformed CSV",
            ),
            Self::CommandTagParseFailed(_) => (
                "Failed to parse database command tag",
                "Check whether the command output format changed",
            ),
            Self::CommandNotFound { command, .. } => {
                (command.not_found_summary(), command.not_found_hint())
            }
            Self::Timeout(_) => (
                "Operation timed out",
                "Retry the operation or increase the timeout",
            ),
            Self::Canceled(_) => ("Operation canceled", "Retry the operation if needed"),
        }
    }

    pub fn summary(&self) -> &'static str {
        self.presentation().0
    }

    pub fn hint(&self) -> &'static str {
        self.presentation().1
    }

    pub fn masked_details(&self) -> String {
        mask_password(self.raw_details().as_ref())
    }

    pub fn user_message(&self) -> String {
        let summary = self.summary();
        let hint = self.hint();
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
        let summary = self.summary();
        let hint = self.hint();
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

    pub(crate) fn raw_details(&self) -> Cow<'_, str> {
        match self {
            Self::ConnectionFailed(details)
            | Self::ConnectionLost(details)
            | Self::PermissionDenied(details)
            | Self::ForeignKeyViolation(details)
            | Self::UniqueViolation(details)
            | Self::LockTimeout(details)
            | Self::ObjectMissing(details)
            | Self::QueryFailed(details)
            | Self::PreviewSizeExceeded(details)
            | Self::UnsupportedOperation(details)
            | Self::UnsupportedOperationWithKind { details, .. }
            | Self::UnsupportedOperationWithSqliteKind { details, .. }
            | Self::ConnectionFailedWithKind { details, .. }
            | Self::MetadataParseFailed(details)
            | Self::EmptyResponse(details)
            | Self::CommandTagParseFailed(details)
            | Self::Timeout(details)
            | Self::Canceled(details)
            | Self::CommandNotFound { details, .. } => Cow::Borrowed(details.as_str()),
            Self::SqlitePath(error) => Cow::Owned(error.to_string()),
            Self::ExportIo(error) => Cow::Owned(error.to_string()),
            Self::InvalidJson(err) => Cow::Owned(err.to_string()),
            Self::CsvParse(err) => Cow::Owned(err.to_string()),
            Self::QueryFailedAfterChange { source, .. } => source.raw_details(),
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
        Self::InvalidJson(Arc::new(e))
    }
}

impl From<csv::Error> for DbOperationError {
    fn from(e: csv::Error) -> Self {
        Self::CsvParse(Arc::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn mysql_connect_timeout_classifier_preserves_errno_and_case_rules() {
        for errno in MYSQL_CONNECT_TIMEOUT_ERRNOS {
            assert!(is_mysql_connect_timeout_message(&format!(
                "Can't connect to MySQL server (host) {errno}"
            )));
        }
        assert!(!is_mysql_connect_timeout_message(
            "Can't connect to MySQL server (host) (111)"
        ));
    }

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
        #[case(DbOperationError::ConnectionFailed("boom".to_string()))]
        #[case(DbOperationError::SqlitePath(SqlitePathError::FileNotFound(
            "/tmp/missing.db".to_string(),
        )))]
        #[case(DbOperationError::ConnectionLost("boom".to_string()))]
        #[case(DbOperationError::PermissionDenied("boom".to_string()))]
        #[case(DbOperationError::ForeignKeyViolation("boom".to_string()))]
        #[case(DbOperationError::UniqueViolation("boom".to_string()))]
        #[case(DbOperationError::LockTimeout("boom".to_string()))]
        #[case(DbOperationError::ObjectMissing("boom".to_string()))]
        #[case(DbOperationError::QueryFailed("boom".to_string()))]
        #[case(DbOperationError::ExportIo(ExportIoSource::new(std::io::Error::other("boom"))))]
        #[case(DbOperationError::UnsupportedOperation("boom".to_string()))]
        #[case(DbOperationError::UnsupportedOperationWithKind {
            kind: UnsupportedOperationKind::ClientVersion,
            details: "boom".to_string(),
        })]
        #[case(DbOperationError::UnsupportedOperationWithSqliteKind {
            kind: SqliteCompatibilityKind::SafeMode,
            details: "boom".to_string(),
        })]
        #[case(DbOperationError::ConnectionFailedWithKind {
            kind: ConnectionFailureKind::TlsHandshake,
            details: "boom".to_string(),
        })]
        #[case(DbOperationError::MetadataParseFailed("boom".to_string()))]
        #[case(DbOperationError::InvalidJson(Arc::new(serde_json::from_str::<i32>("x").unwrap_err())))]
        #[case(DbOperationError::EmptyResponse("boom".to_string()))]
        #[case(
            DbOperationError::CsvParse(Arc::new(csv::Error::from(std::io::Error::other(
                "boom"
            ))))
        )]
        #[case(DbOperationError::CommandTagParseFailed("boom".to_string()))]
        #[case(DbOperationError::CommandNotFound {
            command: DatabaseCli::Psql,
            details: "boom".to_string(),
        })]
        #[case(DbOperationError::Timeout("boom".to_string()))]
        #[case(DbOperationError::Canceled("boom".to_string()))]
        fn non_empty(#[case] error: DbOperationError) {
            assert!(!error.summary().is_empty());
            assert!(!error.hint().is_empty());
            assert!(!error.user_message().is_empty());
        }

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
            let error = DbOperationError::ExportIo(ExportIoSource::new(std::io::Error::other(
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
        #[case(DbOperationError::UniqueViolation("duplicate entry".to_string()))]
        #[case(DbOperationError::ForeignKeyViolation("foreign key failed".to_string()))]
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
            let error = DbOperationError::UniqueViolation(
                "ERROR: duplicate key value violates unique constraint".to_string(),
            );

            assert!(
                error
                    .result_message()
                    .contains("Unique constraint violation.")
            );
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
