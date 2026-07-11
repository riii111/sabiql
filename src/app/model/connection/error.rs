use crate::policy::password_masking::mask_password;
use crate::ports::outbound::DbOperationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionErrorKind {
    CliNotFound,
    SqliteCliNotFound,
    HostUnreachable,
    AuthFailed,
    DatabaseNotFound,
    ConnectionLost,
    Timeout,
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
        {
            return Self::HostUnreachable;
        }

        if stderr_lower.contains("password authentication failed")
            || stderr_lower.contains("authentication failed")
            || (stderr_lower.contains("fatal:") && stderr_lower.contains("password"))
        {
            return Self::AuthFailed;
        }

        if stderr_lower.contains("does not exist")
            && (stderr_lower.contains("database") || stderr_lower.contains("fatal:"))
        {
            return Self::DatabaseNotFound;
        }

        if stderr_lower.contains("timeout expired")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("connection timed out")
        {
            return Self::Timeout;
        }

        if is_connection_lost_message(&stderr_lower) {
            return Self::ConnectionLost;
        }

        Self::Unknown
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::CliNotFound => "Database CLI not found",
            Self::SqliteCliNotFound => "sqlite3 not found",
            Self::HostUnreachable => "Could not resolve host",
            Self::AuthFailed => "Authentication failed",
            Self::DatabaseNotFound => "Database does not exist",
            Self::ConnectionLost => "Connection lost during operation",
            Self::Timeout => "Connection timed out",
            Self::SqliteVersionTooOld => "SQLite 3.37 or later required",
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
            Self::SqliteCliNotFound => "Install sqlite3 and add it to PATH",
            Self::HostUnreachable => "Check the hostname",
            Self::AuthFailed => "Check username and password",
            Self::DatabaseNotFound => "Check database name",
            Self::ConnectionLost => "Reconnect and retry the operation",
            Self::Timeout => "Check network connectivity",
            Self::SqliteVersionTooOld => {
                "Upgrade sqlite3, or open a database without virtual tables"
            }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionErrorInfo {
    pub kind: ConnectionErrorKind,
    masked_details: String,
}

impl ConnectionErrorInfo {
    pub fn new(raw_stderr: impl Into<String>) -> Self {
        let raw_details = raw_stderr.into();
        let kind = ConnectionErrorKind::classify(&raw_details);
        let masked_details = mask_password(&raw_details);

        Self {
            kind,
            masked_details,
        }
    }

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
            DbOperationError::CommandNotFound(details) if details.starts_with("sqlite3:") => {
                ConnectionErrorKind::SqliteCliNotFound
            }
            DbOperationError::CommandNotFound(_) => ConnectionErrorKind::CliNotFound,
            DbOperationError::ConnectionLost(_) => ConnectionErrorKind::ConnectionLost,
            DbOperationError::Timeout(_) => ConnectionErrorKind::Timeout,
            DbOperationError::UnsupportedOperation(details)
                if details.contains("SQLITE_TABLE_LIST_REQUIRED") =>
            {
                ConnectionErrorKind::SqliteVersionTooOld
            }
            DbOperationError::ConnectionFailed(details) => {
                classify_sqlite_path_connection_error(details)
                    .unwrap_or_else(|| ConnectionErrorKind::classify(&raw_details))
            }
            _ => ConnectionErrorKind::Unknown,
        };
        Self::with_kind(kind, raw_details)
    }

    pub fn summary(&self) -> &'static str {
        self.kind.summary()
    }

    pub fn hint(&self) -> &'static str {
        self.kind.hint()
    }

    pub fn masked_details(&self) -> &str {
        &self.masked_details
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
        #[case("Connection refused")]
        #[case("Some random error")]
        #[case("")]
        fn stderr_as_unknown_fallback(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Unknown
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
        #[case(ConnectionErrorKind::DatabaseNotFound)]
        #[case(ConnectionErrorKind::ConnectionLost)]
        #[case(ConnectionErrorKind::Timeout)]
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
        fn new_auto_classifies() {
            let info = ConnectionErrorInfo::new("psql: command not found");
            assert_eq!(info.kind, ConnectionErrorKind::CliNotFound);
        }

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
        fn from_db_operation_error_preserves_connection_lost_kind() {
            let info = ConnectionErrorInfo::from_db_operation_error(
                &DbOperationError::ConnectionLost("connection to server was lost".to_string()),
            );

            assert_eq!(info.kind, ConnectionErrorKind::ConnectionLost);
        }

        #[test]
        fn from_db_operation_error_classifies_sqlite_missing_file() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                    "SQLite database file not found: /tmp/missing.db".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::SqliteFileNotFound);
            assert_eq!(info.summary(), "SQLite database file not found");
        }

        #[test]
        fn from_db_operation_error_classifies_missing_sqlite_cli() {
            let info =
                ConnectionErrorInfo::from_db_operation_error(&DbOperationError::CommandNotFound(
                    "sqlite3: No such file or directory".to_string(),
                ));

            assert_eq!(info.kind, ConnectionErrorKind::SqliteCliNotFound);
            assert_eq!(info.summary(), "sqlite3 not found");
            assert_eq!(info.hint(), "Install sqlite3 and add it to PATH");
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
                &DbOperationError::UnsupportedOperation(
                    "SQLITE_TABLE_LIST_REQUIRED: upgrade sqlite3".to_string(),
                ),
            );

            assert_eq!(info.kind, ConnectionErrorKind::SqliteVersionTooOld);
            assert_eq!(info.summary(), "SQLite 3.37 or later required");
        }

        #[test]
        fn delegates_summary_and_hint() {
            let info = ConnectionErrorInfo::new("psql: command not found");
            assert_eq!(info.summary(), "Database CLI not found");
            assert_eq!(
                info.hint(),
                "Install the database CLI (e.g. psql) and add it to PATH"
            );
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
            let info = ConnectionErrorInfo::new("postgres://user:secret@host");
            assert!(!info.masked_details().contains("secret"));
            assert_eq!(info.masked_details(), "postgres://user:****@host");
        }
    }
}
