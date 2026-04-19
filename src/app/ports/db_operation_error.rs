use std::borrow::Cow;
use std::sync::Arc;

use crate::app::ports::password_masking::mask_password;

pub(crate) fn is_connection_lost_message(lower: &str) -> bool {
    lower.contains("server closed the connection unexpectedly")
        || lower.contains("connection to server was lost")
        || lower.contains("terminating connection")
        || lower.contains("connection not open")
        || lower.contains("broken pipe")
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DbOperationError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Connection lost: {0}")]
    ConnectionLost(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Foreign key constraint violated: {0}")]
    ForeignKeyViolation(String),
    #[error("Unique constraint violated: {0}")]
    UniqueViolation(String),
    #[error("Operation blocked by lock or timeout: {0}")]
    LockTimeout(String),
    #[error("Database object not found: {0}")]
    ObjectMissing(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Metadata parse failed: {0}")]
    MetadataParseFailed(String),
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[source] Arc<serde_json::Error>),
    #[error("Empty response: {0}")]
    EmptyResponse(String),
    #[error("CSV parse error: {0}")]
    CsvParse(#[source] Arc<csv::Error>),
    #[error("Command tag parse failed: {0}")]
    CommandTagParseFailed(String),
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("Operation timed out: {0}")]
    Timeout(String),
}

impl DbOperationError {
    pub fn summary(&self) -> &'static str {
        match self {
            Self::ConnectionFailed(_) => "Connection failed",
            Self::ConnectionLost(_) => "Connection lost during operation",
            Self::PermissionDenied(_) => "Permission denied",
            Self::ForeignKeyViolation(_) => "Foreign key constraint violation",
            Self::UniqueViolation(_) => "Unique constraint violation",
            Self::LockTimeout(_) => "Operation blocked by lock or timeout",
            Self::ObjectMissing(_) => "Database object not found",
            Self::QueryFailed(_) => "Query failed",
            Self::MetadataParseFailed(_) => "Failed to parse database metadata output",
            Self::InvalidJson(_) => "Failed to parse database JSON output",
            Self::EmptyResponse(_) => "Database returned an empty response",
            Self::CsvParse(_) => "Failed to parse database CSV output",
            Self::CommandTagParseFailed(_) => "Failed to parse database command tag",
            Self::CommandNotFound(_) => "Database CLI not found",
            Self::Timeout(_) => "Operation timed out",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::ConnectionFailed(_) => "Check the connection settings and database availability",
            Self::ConnectionLost(_) => "Reconnect and retry the operation",
            Self::PermissionDenied(_) => "Check the connected user's privileges",
            Self::ForeignKeyViolation(_) => {
                "Check referenced rows before retrying the write operation"
            }
            Self::UniqueViolation(_) => "Check for duplicate values before retrying",
            Self::LockTimeout(_) => {
                "Retry; if it persists, check for blocking transactions or timeout settings"
            }
            Self::ObjectMissing(_) => "Check the table, column, or connected database",
            Self::QueryFailed(_) => "Review the database error details and SQL",
            Self::MetadataParseFailed(_) => {
                "Check whether the metadata output format changed unexpectedly"
            }
            Self::InvalidJson(_) => "Check whether the adapter query output shape changed",
            Self::EmptyResponse(_) => "Retry the operation and inspect the command output",
            Self::CsvParse(_) => "Check whether the adapter returned malformed CSV",
            Self::CommandTagParseFailed(_) => "Check whether the command output format changed",
            Self::CommandNotFound(_) => "Install the database client and add it to PATH",
            Self::Timeout(_) => "Retry the operation or increase the timeout",
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
            | Self::MetadataParseFailed(details)
            | Self::EmptyResponse(details)
            | Self::CommandTagParseFailed(details)
            | Self::CommandNotFound(details)
            | Self::Timeout(details) => Cow::Borrowed(details.as_str()),
            Self::InvalidJson(err) => Cow::Owned(err.to_string()),
            Self::CsvParse(err) => Cow::Owned(err.to_string()),
        }
    }

    pub fn masked_details(&self) -> String {
        mask_password(self.raw_details().as_ref())
    }

    pub fn user_message(&self) -> String {
        let summary = self.summary();
        let hint = self.hint();
        let details = self.masked_details();

        match (details.trim().is_empty(), hint.is_empty()) {
            (true, true) => summary.to_string(),
            (true, false) => format!("{summary}. {hint}."),
            (false, true) => format!("{summary}: {details}"),
            (false, false) => format!("{summary}: {details}. {hint}."),
        }
    }

    pub fn result_message(&self) -> String {
        let summary = self.summary();
        let hint = self.hint();
        let details = self.masked_details();

        match (details.trim().is_empty(), hint.is_empty()) {
            (true, true) => summary.to_string(),
            (true, false) => format!("{summary}. {hint}."),
            (false, true) => format!("{summary}\n\nDetails:\n{details}"),
            (false, false) => format!("{summary}. {hint}.\n\nDetails:\n{details}"),
        }
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

    mod summaries_and_hints {
        use super::*;

        #[rstest]
        #[case(DbOperationError::ConnectionFailed("boom".to_string()))]
        #[case(DbOperationError::ConnectionLost("boom".to_string()))]
        #[case(DbOperationError::PermissionDenied("boom".to_string()))]
        #[case(DbOperationError::ForeignKeyViolation("boom".to_string()))]
        #[case(DbOperationError::UniqueViolation("boom".to_string()))]
        #[case(DbOperationError::LockTimeout("boom".to_string()))]
        #[case(DbOperationError::ObjectMissing("boom".to_string()))]
        #[case(DbOperationError::QueryFailed("boom".to_string()))]
        #[case(DbOperationError::MetadataParseFailed("boom".to_string()))]
        #[case(DbOperationError::InvalidJson(Arc::new(serde_json::from_str::<i32>("x").unwrap_err())))]
        #[case(DbOperationError::EmptyResponse("boom".to_string()))]
        #[case(
            DbOperationError::CsvParse(Arc::new(csv::Error::from(std::io::Error::other(
                "boom"
            ))))
        )]
        #[case(DbOperationError::CommandTagParseFailed("boom".to_string()))]
        #[case(DbOperationError::CommandNotFound("boom".to_string()))]
        #[case(DbOperationError::Timeout("boom".to_string()))]
        fn non_empty(#[case] error: DbOperationError) {
            assert!(!error.summary().is_empty());
            assert!(!error.hint().is_empty());
            assert!(!error.user_message().is_empty());
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
            DbOperationError::ConnectionFailed("postgres://user:p@ss@host".to_string()),
            "postgres://user:****@host"
        )]
        fn hides_passwords(#[case] error: DbOperationError, #[case] expected: &str) {
            assert_eq!(error.masked_details(), expected);
        }
    }

    mod user_messages {
        use super::*;

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
    }
}
