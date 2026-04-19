use std::borrow::Cow;
use std::sync::Arc;

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
            Self::LockTimeout(_) => "Retry after the blocking transaction finishes",
            Self::ObjectMissing(_) => "Check the table, column, or connected database",
            Self::QueryFailed(_) => "Review the database error details and SQL",
            Self::InvalidJson(_) => "Check whether the adapter query output shape changed",
            Self::EmptyResponse(_) => "Retry the operation and inspect the command output",
            Self::CsvParse(_) => "Check whether the adapter returned malformed CSV",
            Self::CommandTagParseFailed(_) => "Check whether the command output format changed",
            Self::CommandNotFound(_) => "Install psql and add it to PATH",
            Self::Timeout(_) => "Retry the operation or increase the timeout",
        }
    }

    pub fn raw_details(&self) -> Cow<'_, str> {
        match self {
            Self::ConnectionFailed(details)
            | Self::ConnectionLost(details)
            | Self::PermissionDenied(details)
            | Self::ForeignKeyViolation(details)
            | Self::UniqueViolation(details)
            | Self::LockTimeout(details)
            | Self::ObjectMissing(details)
            | Self::QueryFailed(details)
            | Self::EmptyResponse(details)
            | Self::CommandTagParseFailed(details)
            | Self::CommandNotFound(details)
            | Self::Timeout(details) => Cow::Borrowed(details.as_str()),
            Self::InvalidJson(err) => Cow::Owned(err.to_string()),
            Self::CsvParse(err) => Cow::Owned(err.to_string()),
        }
    }

    pub fn masked_details(&self) -> String {
        Self::mask_password(self.raw_details().as_ref())
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::QueryFailed(details) if !details.trim().is_empty() => self.masked_details(),
            _ => {
                let summary = self.summary();
                let hint = self.hint();
                if hint.is_empty() {
                    summary.to_string()
                } else {
                    format!("{summary}. {hint}.")
                }
            }
        }
    }

    pub fn result_message(&self) -> String {
        if let Self::QueryFailed(_) = self {
            self.user_message()
        } else {
            let details = self.masked_details();
            if details.trim().is_empty() {
                self.user_message()
            } else {
                format!("{}\n\nDetails:\n{}", self.user_message(), details)
            }
        }
    }

    fn mask_password(text: &str) -> String {
        let result = Self::mask_url_passwords(text);
        let result = Self::mask_kv_passwords(&result);
        Self::mask_env_passwords(&result)
    }

    fn mask_url_passwords(text: &str) -> String {
        let lower = text.to_lowercase();
        let mut result = String::with_capacity(text.len());
        let mut i = 0;

        while i < text.len() {
            let remaining = &lower[i..];
            let scheme_len = if remaining.starts_with("postgresql://") {
                "postgresql://".len()
            } else if remaining.starts_with("postgres://") {
                "postgres://".len()
            } else if remaining.starts_with("mysql://") {
                "mysql://".len()
            } else {
                0
            };

            if scheme_len > 0 {
                let after_scheme = i + scheme_len;
                if let Some(colon_off) = text[after_scheme..].find(':') {
                    let colon = after_scheme + colon_off;
                    if let Some(at_off) = text[(colon + 1)..].find('@') {
                        let at = colon + 1 + at_off;
                        result.push_str(&text[i..=colon]);
                        result.push_str("****");
                        i = at;
                        continue;
                    }
                }
            }

            let ch = text[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }

        result
    }

    fn mask_kv_passwords(text: &str) -> String {
        let lower = text.to_lowercase();
        Self::mask_after_prefix(text, |pos| {
            let needle = "password=";
            lower[pos..].starts_with(needle).then_some(needle.len())
        })
    }

    fn mask_env_passwords(text: &str) -> String {
        const PREFIXES: &[&str] = &["PGPASSWORD=", "MYSQL_PASSWORD=", "MYSQL_PWD="];
        Self::mask_after_prefix(text, |pos| {
            PREFIXES
                .iter()
                .find_map(|p| text[pos..].starts_with(p).then_some(p.len()))
        })
    }

    fn mask_after_prefix(text: &str, find_prefix: impl Fn(usize) -> Option<usize>) -> String {
        let mut result = String::with_capacity(text.len());
        let mut i = 0;

        while i < text.len() {
            if let Some(prefix_len) = find_prefix(i) {
                let eq_end = i + prefix_len;
                result.push_str(&text[i..eq_end]);
                result.push_str("****");
                let mut j = eq_end;
                while j < text.len() && !text.as_bytes()[j].is_ascii_whitespace() {
                    j += 1;
                }
                i = j;
            } else {
                let ch = text[i..].chars().next().unwrap();
                result.push(ch);
                i += ch.len_utf8();
            }
        }

        result
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
        #[case(DbOperationError::EmptyResponse("boom".to_string()))]
        #[case(DbOperationError::CommandTagParseFailed("boom".to_string()))]
        #[case(DbOperationError::CommandNotFound("boom".to_string()))]
        #[case(DbOperationError::Timeout("boom".to_string()))]
        fn non_empty(#[case] error: DbOperationError) {
            assert!(!error.summary().is_empty());
            assert!(!error.hint().is_empty());
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
                "Permission denied. Check the connected user's privileges."
            );
        }

        #[test]
        fn generic_query_failed_uses_raw_details() {
            let error = DbOperationError::QueryFailed("syntax error at or near SELECT".to_string());

            assert_eq!(error.user_message(), "syntax error at or near SELECT");
        }

        #[test]
        fn result_message_keeps_details_for_actionable_errors() {
            let error = DbOperationError::UniqueViolation(
                "ERROR: duplicate key value violates unique constraint".to_string(),
            );

            assert!(error.result_message().contains("Unique constraint violation."));
            assert!(error.result_message().contains("Details:"));
        }
    }
}
