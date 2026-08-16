use std::io::{self, Write};

use crate::app::ports::outbound::DbOperationError;

use super::probe::{is_mysql_connect_timeout_message, mysql_tls_failure_kind, validate_sql_mode};
use super::xml::MySqlResultSet;

pub(super) fn has_mysql_cli_error(output: &[u8]) -> bool {
    output
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .any(|line| {
            let mut line = line;
            while line
                .first()
                .is_some_and(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            {
                line = &line[1..];
            }
            line.starts_with(b"ERROR ")
                && line
                    .get(6..10)
                    .is_some_and(|code| code.iter().all(u8::is_ascii_digit))
        })
}

pub(super) fn is_mysql_batch_diagnostic(line: &[u8]) -> bool {
    line.starts_with(b"mysql: ") || line.starts_with(b"Warning: ")
}

pub(super) fn trace_mysql_error(output: &[u8]) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() && has_mysql_cli_error(output) {
        write_mysql_transcript_line("sabiql mysql frame: ERROR line observed");
    }
}

pub(super) fn write_mysql_transcript_line(line: &str) {
    let mut stderr = io::stderr();
    let _ = stderr.write_all(line.as_bytes());
    let _ = stderr.write_all(b"\n");
}

pub(super) fn validate_mode_probe(
    result: &MySqlResultSet,
    marker: &str,
) -> Result<(), DbOperationError> {
    if result.values.len() != 1 || result.columns != ["__sabiql_probe", "__sabiql_sql_mode"] {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe returned an unexpected result".to_string(),
        ));
    }
    let values = &result.values[0];
    if values.len() != 2 {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe returned an unexpected result".to_string(),
        ));
    }
    if values[0].as_str() != Some(marker) {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe marker did not match".to_string(),
        ));
    }
    let mode = values[1].as_str().ok_or_else(|| {
        DbOperationError::QueryFailed("mysql sql_mode probe returned no mode".to_string())
    })?;
    validate_sql_mode(mode)
}

pub(super) fn classify_mysql_query_failure(stderr: &[u8]) -> DbOperationError {
    let details = clean_mysql_stderr(stderr, "mysql query failed");
    let lower = details.to_ascii_lowercase();
    let error_code = mysql_server_error_code(&lower);
    if (error_code.is_none() || error_code == Some(2026))
        && let Some(kind) = mysql_tls_failure_kind(&lower)
    {
        DbOperationError::ConnectionFailedWithKind { kind, details }
    } else if let Some(error_code) = error_code {
        classify_mysql_server_error(error_code, &details, &lower)
            .unwrap_or(DbOperationError::QueryFailed(details))
    } else if is_mysql_connect_timeout_message(&details)
        || lower.contains("connect timeout")
        || lower.contains("connection timed out")
    {
        DbOperationError::Timeout(details)
    } else if lower.contains("command denied") {
        DbOperationError::PermissionDenied(details)
    } else if lower.contains("unknown database") {
        DbOperationError::ConnectionFailed(details)
    } else if lower.contains("doesn't exist") || lower.contains("does not exist") {
        DbOperationError::ObjectMissing(details)
    } else if lower.contains("access denied") || lower.contains("authentication") {
        DbOperationError::ConnectionFailed(details)
    } else if lower.contains("lost connection") || lower.contains("server has gone away") {
        DbOperationError::ConnectionLost(details)
    } else if lower.contains("lock wait timeout") || lower.contains("deadlock found") {
        DbOperationError::LockTimeout(details)
    } else if lower.contains("foreign key constraint") {
        DbOperationError::ForeignKeyViolation(details)
    } else if lower.contains("duplicate entry") {
        DbOperationError::UniqueViolation(details)
    } else if lower.contains("query execution was interrupted")
        || lower.contains("query was interrupted")
    {
        DbOperationError::Canceled(details)
    } else {
        DbOperationError::QueryFailed(details)
    }
}

fn classify_mysql_server_error(
    error_code: u32,
    details: &str,
    lowercase_details: &str,
) -> Option<DbOperationError> {
    let error = |constructor: fn(String) -> DbOperationError| constructor(details.to_string());

    Some(match error_code {
        1022 | 1062 => error(DbOperationError::UniqueViolation),
        1044 | 1142 | 1143 | 1227 => error(DbOperationError::PermissionDenied),
        1045 | 1049 => error(DbOperationError::ConnectionFailed),
        1051 | 1054 | 1109 | 1146 => error(DbOperationError::ObjectMissing),
        1205 | 1213 => error(DbOperationError::LockTimeout),
        1215 | 1216 | 1217 | 1451 | 1452 => error(DbOperationError::ForeignKeyViolation),
        1317 => error(DbOperationError::Canceled),
        2006 | 2013 => error(DbOperationError::ConnectionLost),
        2003 if is_mysql_connect_timeout_message(details)
            || lowercase_details.contains("connect timeout")
            || lowercase_details.contains("connection timed out") =>
        {
            DbOperationError::Timeout(details.to_string())
        }
        2003 => DbOperationError::ConnectionFailed(details.to_string()),
        _ => return None,
    })
}

fn mysql_server_error_code(lowercase_details: &str) -> Option<u32> {
    let marker = "error ";
    let start = lowercase_details.find(marker)? + marker.len();
    let digits = &lowercase_details[start..];
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

fn clean_mysql_stderr(stderr: &[u8], fallback: &str) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::outbound::ConnectionFailureKind;

    #[test]
    fn classifies_mysql_query_failures_by_server_error() {
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1045 (28000): Access denied for user 'app'@'localhost' (using password: YES)"
            ),
            DbOperationError::ConnectionFailed(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (60)"
            ),
            DbOperationError::Timeout(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (111)"
            ),
            DbOperationError::ConnectionFailed(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1049 (42000): schema selection failed"),
            DbOperationError::ConnectionFailed(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1054 (42S22): column lookup failed"),
            DbOperationError::ObjectMissing(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1051 (42S02): generic failure"),
            DbOperationError::ObjectMissing(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1109 (42S02): generic failure"),
            DbOperationError::ObjectMissing(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1142 (42000): command denied to user"),
            DbOperationError::PermissionDenied(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1044 (42000): Access denied for user 'app' to database 'app'"
            ),
            DbOperationError::PermissionDenied(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1146 (42S02): table lookup failed"),
            DbOperationError::ObjectMissing(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1205 (HY000): Lock wait timeout exceeded"),
            DbOperationError::LockTimeout(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1213 (40001): generic failure"),
            DbOperationError::LockTimeout(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1022 (23000): generic failure"),
            DbOperationError::UniqueViolation(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1062 (23000): generic failure"),
            DbOperationError::UniqueViolation(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1452 (23000): Cannot add or update a child row: a foreign key constraint fails"
            ),
            DbOperationError::ForeignKeyViolation(_)
        ));
        let masked = classify_mysql_query_failure(b"ERROR password=secret");
        assert!(!masked.masked_details().contains("secret"));
    }

    #[test]
    fn waits_for_a_mysql_error_code_before_matching_a_partial_stderr_line() {
        assert!(!has_mysql_cli_error(b"ERROR"));
        assert!(!has_mysql_cli_error(b"ERROR 1"));
        assert!(has_mysql_cli_error(b"ERROR 1054"));
    }

    #[test]
    fn does_not_use_wording_to_classify_an_unknown_server_error_code() {
        let error = classify_mysql_query_failure(
            b"ERROR 9999 (HY000): Duplicate entry duplicate_value for key PRIMARY",
        );

        assert!(matches!(error, DbOperationError::QueryFailed(_)));

        let error = classify_mysql_query_failure(b"ERROR 9999 (HY000): SSL connection error");
        assert!(matches!(error, DbOperationError::QueryFailed(_)));
    }

    #[test]
    fn uses_wording_when_mysql_error_code_is_not_present() {
        assert!(matches!(
            classify_mysql_query_failure(b"duplicate entry duplicate_value for key PRIMARY"),
            DbOperationError::UniqueViolation(_)
        ));
    }

    #[test]
    fn classifies_mysql_tls_query_failures_as_connection_errors() {
        let error = classify_mysql_query_failure(
            b"ERROR 2026 (HY000): SSL connection error: error:0A000086:SSL routines::certificate verify failed",
        );

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::TlsCertificateVerification,
                ..
            }
        ));
    }
}
