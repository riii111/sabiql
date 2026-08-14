use std::io::{self, Write};

use crate::app::ports::outbound::DbOperationError;

use super::probe::{is_mysql_connect_timeout_message, validate_sql_mode};
use super::xml::MysqlResultSet;

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
            line.starts_with(b"ERROR ") || line == b"ERROR"
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
    result: &MysqlResultSet,
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
    if is_mysql_tls_error(&lower) {
        DbOperationError::ConnectionFailed(details)
    } else if is_mysql_connect_timeout_message(&details)
        || lower.contains("connect timeout")
        || lower.contains("connection timed out")
    {
        DbOperationError::Timeout(details)
    } else if matches!(error_code, Some(1044 | 1142 | 1143 | 1227))
        || lower.contains("command denied")
    {
        DbOperationError::PermissionDenied(details)
    } else if error_code == Some(1045)
        || lower.contains("access denied")
        || lower.contains("authentication")
    {
        DbOperationError::ConnectionFailed(details)
    } else if lower.contains("lost connection") || lower.contains("server has gone away") {
        DbOperationError::ConnectionLost(details)
    } else if lower.contains("lock wait timeout") || lower.contains("deadlock found") {
        DbOperationError::LockTimeout(details)
    } else if matches!(error_code, Some(1215 | 1216 | 1217 | 1451 | 1452))
        || lower.contains("foreign key constraint")
    {
        DbOperationError::ForeignKeyViolation(details)
    } else if lower.contains("doesn't exist") || lower.contains("does not exist") {
        DbOperationError::ObjectMissing(details)
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

fn is_mysql_tls_error(lowercase_details: &str) -> bool {
    [
        "error 2026",
        "tls/ssl error",
        "ssl connection error",
        "ssl handshake",
        "tls handshake",
        "handshake failure",
        "tlsv1 alert",
        "certificate verify failed",
        "certificate verification failure",
        "certificate validation failure",
        "unable to get local issuer",
        "self-signed certificate",
        "unknown ca",
        "certificate required",
        "peer did not return a certificate",
    ]
    .iter()
    .any(|marker| lowercase_details.contains(marker))
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
#[path = "error_tests.rs"]
mod tests;
