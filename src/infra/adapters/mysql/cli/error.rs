use crate::app::ports::outbound::DbOperationError;

pub(crate) fn clean_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        "mysql probe failed".to_string()
    } else {
        text
    }
}

pub(crate) fn classify_mysql_probe_failure(stderr: String) -> DbOperationError {
    if is_mysql_connect_timeout_message(&stderr) {
        DbOperationError::Timeout(stderr)
    } else {
        DbOperationError::ConnectionFailed(stderr)
    }
}

pub(crate) fn is_mysql_connect_timeout_message(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("can't connect to mysql server")
        && (lower.contains("(110)") || lower.contains("(10060)"))
}

pub(crate) fn classify_mysql_query_failure(stderr: &[u8]) -> DbOperationError {
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
