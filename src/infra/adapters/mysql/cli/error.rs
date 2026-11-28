use std::io::{self, Write};

use crate::app::ports::outbound::{ConnectionFailureKind, DatabaseCli, DbOperationError};

use super::probe::mysql_tls_failure_kind;

const MYSQL_CONNECT_TIMEOUT_ERRNOS: [&str; 3] = ["(60)", "(110)", "(10060)"];

fn mysql_server_error_code(lowercase_details: &str) -> Option<u32> {
    let start = lowercase_details.find("error ")? + "error ".len();
    let digits = &lowercase_details[start..];
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

pub(in crate::adapters::mysql) fn is_mysql_missing_trigger(error: &DbOperationError) -> bool {
    matches!(error, DbOperationError::QueryFailed(details)
        if mysql_server_error_code(&details.to_ascii_lowercase()) == Some(1360))
}

pub(super) fn is_mysql_connect_timeout_message(value: &str) -> bool {
    is_mysql_connect_timeout(&value.to_ascii_lowercase())
}

fn is_mysql_connect_timeout(lowercase: &str) -> bool {
    lowercase.contains("can't connect to mysql server")
        && MYSQL_CONNECT_TIMEOUT_ERRNOS
            .iter()
            .any(|errno| lowercase.contains(errno))
}

pub(super) fn map_mysql_cli_spawn_error(error: io::Error) -> DbOperationError {
    if error.kind() == io::ErrorKind::NotFound {
        DbOperationError::CommandNotFound {
            command: DatabaseCli::MySql,
            details: error.to_string(),
        }
    } else {
        DbOperationError::ConnectionFailed(error.to_string())
    }
}

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

pub(super) fn has_complete_mysql_cli_error(output: &[u8]) -> bool {
    output
        .split_inclusive(|byte| *byte == b'\n' || *byte == b'\r')
        .any(|line| {
            line.last()
                .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
                && has_mysql_cli_error(line)
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

pub(super) fn classify_mysql_query_failure(stderr: &[u8]) -> DbOperationError {
    classify_mysql_query_failure_with_packet_limit(stderr, None)
}

pub(super) fn classify_mysql_query_failure_with_packet_limit(
    stderr: &[u8],
    client_packet_limit_bytes: Option<usize>,
) -> DbOperationError {
    let details = clean_mysql_stderr(stderr, "mysql query failed");
    let lower = details.to_ascii_lowercase();
    let error_code = mysql_server_error_code(&lower);
    if error_code == Some(2020)
        && let Some(client_packet_limit_bytes) = client_packet_limit_bytes
    {
        return DbOperationError::QueryFailed(format!(
            "MySQL protocol packet exceeds the {client_packet_limit_bytes}-byte client limit"
        ));
    }
    if matches!(error_code, None | Some(2026))
        && let Some(kind) = mysql_tls_failure_kind(&lower)
    {
        return connection_failed_with_kind(kind, details);
    }

    let wording = |phrase| error_code.is_none() && lower.contains(phrase);
    match error_code {
        code if code == Some(3024)
            || (matches!(code, None | Some(2003))
                && (is_mysql_connect_timeout(&lower)
                    || lower.contains("connect timeout")
                    || lower.contains("connection timed out"))) =>
        {
            DbOperationError::Timeout(details)
        }
        code if matches!(code, Some(1044 | 1142 | 1143 | 1227)) || wording("command denied") => {
            DbOperationError::PermissionDenied(details)
        }
        code if code == Some(1049) || wording("unknown database") => {
            connection_failed_with_kind(ConnectionFailureKind::DatabaseNotFound, details)
        }
        code if matches!(code, Some(1051 | 1054 | 1109 | 1146))
            || wording("doesn't exist")
            || wording("does not exist") =>
        {
            DbOperationError::ObjectMissing(details)
        }
        code if code == Some(1045) || wording("access denied") || wording("authentication") => {
            connection_failed_with_kind(ConnectionFailureKind::Auth, details)
        }
        code if matches!(code, Some(2006 | 2013))
            || wording("lost connection")
            || wording("server has gone away") =>
        {
            DbOperationError::ConnectionLost(details)
        }
        code if matches!(code, Some(1205 | 1213))
            || wording("lock wait timeout")
            || wording("deadlock found") =>
        {
            DbOperationError::LockTimeout(details)
        }
        code if code == Some(1317)
            || wording("query execution was interrupted")
            || wording("query was interrupted") =>
        {
            DbOperationError::Canceled(details)
        }
        Some(2003) => {
            connection_failed_with_kind(ConnectionFailureKind::ConnectionRefused, details)
        }
        Some(2005) => connection_failed_with_kind(ConnectionFailureKind::HostUnreachable, details),
        _ => DbOperationError::QueryFailed(details),
    }
}

fn connection_failed_with_kind(kind: ConnectionFailureKind, details: String) -> DbOperationError {
    DbOperationError::ConnectionFailedWithKind { kind, details }
}

pub(super) fn clean_mysql_stderr(stderr: &[u8], fallback: &str) -> String {
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

    #[test]
    fn classifies_mysql_query_failures_by_server_error() {
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1045 (28000): Access denied for user 'app'@'localhost' (using password: YES)"
            ),
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::Auth,
                ..
            }
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
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::ConnectionRefused,
                ..
            }
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1049 (42000): schema selection failed"),
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::DatabaseNotFound,
                ..
            }
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
        let packet_error = classify_mysql_query_failure_with_packet_limit(
            b"ERROR 1153 (08S01): Got a packet bigger than 'max_allowed_packet' bytes",
            Some(33_554_432),
        );
        assert!(matches!(
            packet_error,
            DbOperationError::QueryFailed(details)
                if details.contains("Got a packet bigger than 'max_allowed_packet' bytes")
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 2020 (HY000): Got packet bigger than 'max_allowed_packet' bytes"),
            DbOperationError::QueryFailed(details)
                if details.contains("Got packet bigger than 'max_allowed_packet' bytes")
        ));
        assert!(matches!(
            classify_mysql_query_failure_with_packet_limit(
                b"ERROR 2020 (HY000): Got packet bigger than 'max_allowed_packet' bytes",
                Some(33_554_432),
            ),
            DbOperationError::QueryFailed(details)
                if details == "MySQL protocol packet exceeds the 33554432-byte client limit"
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 2013 (HY000): Lost connection to MySQL server during query"
            ),
            DbOperationError::ConnectionLost(_)
        ));
        let masked = classify_mysql_query_failure(b"ERROR password=secret");
        assert!(!masked.masked_details().contains("secret"));
    }

    #[test]
    fn classifies_mysql_unknown_host_errno_as_host_unreachable() {
        let details = "ERROR 2005 (HY000): Unknown MySQL server host 'db.internal' (11001)";

        assert!(matches!(
            classify_mysql_query_failure(details.as_bytes()),
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::HostUnreachable,
                details: actual_details,
            } if actual_details == details
        ));
    }

    #[test]
    fn classifies_mysql_unknown_database_without_error_code_as_database_not_found() {
        let details = "mysql: [ERROR] Unknown database 'missing'";

        assert!(matches!(
            classify_mysql_query_failure(details.as_bytes()),
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::DatabaseNotFound,
                details: actual_details,
            } if actual_details == details
        ));
    }

    #[test]
    fn preserves_mysql_query_timeout_code_and_message_in_timeout_details() {
        let details = "ERROR 3024 (HY000): Query execution was interrupted, maximum statement execution time exceeded";

        assert!(matches!(
            classify_mysql_query_failure(details.as_bytes()),
            DbOperationError::Timeout(actual_details) if actual_details == details
        ));
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

        let error = classify_mysql_query_failure(b"ERROR 9999 (HY000): Unknown database 'missing'");
        assert!(matches!(error, DbOperationError::QueryFailed(_)));
    }

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

    #[test]
    fn uses_wording_when_mysql_error_code_is_not_present() {
        assert!(matches!(
            classify_mysql_query_failure(b"mysql: [ERROR] Lost connection to MySQL server"),
            DbOperationError::ConnectionLost(_)
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
