use crate::app::ports::outbound::{ConnectionFailureKind, DatabaseCli, DbOperationError};

pub(in crate::adapters::postgres) fn classify_cli_spawn_error(
    error: std::io::Error,
) -> DbOperationError {
    if error.kind() == std::io::ErrorKind::NotFound {
        DbOperationError::CommandNotFound {
            command: DatabaseCli::Psql,
            details: error.to_string(),
        }
    } else {
        DbOperationError::QueryFailed(error.to_string())
    }
}

pub(in crate::adapters::postgres) fn classify_query_error(stderr: &str) -> DbOperationError {
    let trimmed = stderr.trim();
    let Some(details) = (!trimmed.is_empty()).then_some(trimmed) else {
        return DbOperationError::QueryFailed(String::new());
    };

    if let Some(sqlstate) = extract_sqlstate(details) {
        return classify_by_sqlstate(sqlstate, details);
    }

    classify_by_stderr(details)
}

fn classify_by_sqlstate(sqlstate: &str, details: &str) -> DbOperationError {
    match sqlstate {
        "08003" | "08006" | "08P01" | "57P01" | "57P02" => {
            DbOperationError::ConnectionLost(details.to_string())
        }
        "08000" | "08001" | "08004" | "08007" => classify_connection_failure(details),
        "28000" | "28P01" => connection_failed_with_kind(ConnectionFailureKind::Auth, details),
        "3D000" => connection_failed_with_kind(ConnectionFailureKind::DatabaseNotFound, details),
        "25006" | "42501" => DbOperationError::PermissionDenied(details.to_string()),
        "23503" => DbOperationError::ForeignKeyViolation(details.to_string()),
        "23505" => DbOperationError::UniqueViolation(details.to_string()),
        "40001" | "40P01" | "55P03" => DbOperationError::LockTimeout(details.to_string()),
        "57014" => classify_query_canceled(details),
        "42P01" | "42703" => DbOperationError::ObjectMissing(details.to_string()),
        code if code.starts_with("08") => {
            if is_connection_lost_message(&details.to_lowercase()) {
                DbOperationError::ConnectionLost(details.to_string())
            } else {
                classify_connection_failure(details)
            }
        }
        _ => DbOperationError::QueryFailed(details.to_string()),
    }
}

fn classify_by_stderr(details: &str) -> DbOperationError {
    let lower = details.to_lowercase();

    if lower.contains("permission denied") || lower.contains("must be owner of") {
        return DbOperationError::PermissionDenied(details.to_string());
    }

    if let Some(kind) = connection_failure_kind(&lower) {
        return connection_failed_with_kind(kind, details);
    }

    if lower.contains("connection refused") {
        return connection_failed_with_kind(ConnectionFailureKind::ConnectionRefused, details);
    }

    if lower.contains("could not connect to server") {
        return DbOperationError::ConnectionFailed(details.to_string());
    }

    if lower.contains("violates foreign key constraint") || lower.contains("foreign key constraint")
    {
        return DbOperationError::ForeignKeyViolation(details.to_string());
    }

    if lower.contains("duplicate key value")
        || lower.contains("violates unique constraint")
        || lower.contains("unique constraint")
    {
        return DbOperationError::UniqueViolation(details.to_string());
    }

    if lower.contains("lock not available")
        || lower.contains("canceling statement due to lock timeout")
    {
        return DbOperationError::LockTimeout(details.to_string());
    }

    if lower.contains("canceling statement due to statement timeout")
        || lower.contains("statement timeout")
        || lower.contains("query timed out")
    {
        return DbOperationError::Timeout(details.to_string());
    }

    if lower.contains("canceling statement due to user request") || lower.contains("query canceled")
    {
        return DbOperationError::Canceled(details.to_string());
    }

    if is_missing_object(&lower) {
        return DbOperationError::ObjectMissing(details.to_string());
    }

    if is_connection_lost_message(&lower) {
        return DbOperationError::ConnectionLost(details.to_string());
    }

    DbOperationError::QueryFailed(details.to_string())
}

fn is_missing_database_or_role(lower: &str) -> bool {
    lower.contains("fatal:")
        && lower.contains("does not exist")
        && (lower.contains("database") || lower.contains("role"))
}

fn classify_connection_failure(details: &str) -> DbOperationError {
    let lower = details.to_lowercase();
    if let Some(kind) = connection_failure_kind(&lower) {
        connection_failed_with_kind(kind, details)
    } else if lower.contains("timeout expired") || lower.contains("timed out") {
        DbOperationError::Timeout(details.to_string())
    } else if lower.contains("connection refused") {
        connection_failed_with_kind(ConnectionFailureKind::ConnectionRefused, details)
    } else if is_connection_lost_message(&lower) {
        DbOperationError::ConnectionLost(details.to_string())
    } else {
        DbOperationError::ConnectionFailed(details.to_string())
    }
}

fn connection_failure_kind(lower: &str) -> Option<ConnectionFailureKind> {
    if lower.contains("could not translate host name")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname provided")
    {
        Some(ConnectionFailureKind::HostUnreachable)
    } else if lower.contains("password authentication failed")
        || lower.contains("authentication failed")
    {
        Some(ConnectionFailureKind::Auth)
    } else if is_missing_database_or_role(lower) {
        Some(ConnectionFailureKind::DatabaseNotFound)
    } else {
        None
    }
}

fn connection_failed_with_kind(kind: ConnectionFailureKind, details: &str) -> DbOperationError {
    DbOperationError::ConnectionFailedWithKind {
        kind,
        details: details.to_string(),
    }
}

fn is_missing_object(lower: &str) -> bool {
    (lower.contains("does not exist")
        && (lower.contains("relation")
            || lower.contains("column")
            || lower.contains("table")
            || lower.contains("schema")))
        || lower.contains("undefined column")
}

fn classify_query_canceled(details: &str) -> DbOperationError {
    let lower = details.to_lowercase();
    if lower.contains("lock timeout") || lower.contains("lock not available") {
        DbOperationError::LockTimeout(details.to_string())
    } else if lower.contains("user request") || lower.contains("query canceled") {
        DbOperationError::Canceled(details.to_string())
    } else {
        DbOperationError::Timeout(details.to_string())
    }
}

fn is_connection_lost_message(lower: &str) -> bool {
    lower.contains("server closed the connection unexpectedly")
        || lower.contains("connection to server was lost")
        || lower.contains("terminating connection")
        || lower.contains("connection not open")
        || lower.contains("broken pipe")
}

fn extract_sqlstate(details: &str) -> Option<&str> {
    details
        .lines()
        .find_map(extract_verbose_sqlstate)
        .or_else(|| details.lines().find_map(extract_named_sqlstate))
}

fn extract_verbose_sqlstate(line: &str) -> Option<&str> {
    for prefix in ["ERROR:", "FATAL:", "PANIC:"] {
        let Some(prefix_pos) = line.find(prefix) else {
            continue;
        };
        let rest = &line[(prefix_pos + prefix.len())..];
        let rest = rest.trim_start();
        let Some(code) = rest.get(..5) else {
            continue;
        };
        if is_sqlstate(code) && rest.as_bytes().get(5) == Some(&b':') {
            return Some(code);
        }
    }
    None
}

fn extract_named_sqlstate(line: &str) -> Option<&str> {
    for prefix in ["SQL state:", "SQLSTATE:"] {
        let Some(rest) = line.trim_start().strip_prefix(prefix) else {
            continue;
        };
        let code = rest.split_whitespace().next()?;
        if is_sqlstate(code) {
            return Some(code);
        }
    }
    None
}

fn is_sqlstate(code: &str) -> bool {
    code.len() == 5
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod sqlstate {
        use super::*;

        #[rstest]
        #[case("ERROR:  42501: permission denied for table users", "42501")]
        #[case(
            "FATAL:  23505: duplicate key value violates unique constraint",
            "23505"
        )]
        #[case("psql:/tmp/f.sql:1: ERROR:  42501: permission denied", "42501")]
        #[case("SQL state: 42P01", "42P01")]
        fn extracts_codes(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(extract_sqlstate(input), Some(expected));
        }

        #[test]
        fn non_ascii_message_does_not_panic() {
            assert_eq!(
                extract_verbose_sqlstate("ERROR:  エラーが発生しました"),
                None
            );
        }
    }

    mod classification {
        use super::*;

        fn classification_name(error: DbOperationError) -> &'static str {
            match error {
                DbOperationError::PermissionDenied(_) => "PermissionDenied",
                DbOperationError::ForeignKeyViolation(_) => "ForeignKeyViolation",
                DbOperationError::UniqueViolation(_) => "UniqueViolation",
                DbOperationError::LockTimeout(_) => "LockTimeout",
                DbOperationError::Timeout(_) => "Timeout",
                DbOperationError::ObjectMissing(_) => "ObjectMissing",
                DbOperationError::ConnectionLost(_) => "ConnectionLost",
                DbOperationError::ConnectionFailed(_) => "ConnectionFailed",
                DbOperationError::ConnectionFailedWithKind { kind, .. } => match kind {
                    ConnectionFailureKind::HostUnreachable => "HostUnreachable",
                    ConnectionFailureKind::Auth => "Auth",
                    ConnectionFailureKind::DatabaseNotFound => "DatabaseNotFound",
                    ConnectionFailureKind::ConnectionRefused => "ConnectionRefused",
                    _ => "TypedConnectionFailure",
                },
                DbOperationError::QueryFailed(_) => "QueryFailed",
                _ => "Other",
            }
        }

        #[rstest]
        #[case("ERROR:  42501: permission denied for table users", "PermissionDenied")]
        #[case(
            "ERROR:  25006: cannot execute in a read-only transaction",
            "PermissionDenied"
        )]
        #[case(
            "ERROR:  23503: insert or update on table violates foreign key constraint",
            "ForeignKeyViolation"
        )]
        #[case(
            "ERROR:  23505: duplicate key value violates unique constraint",
            "UniqueViolation"
        )]
        #[case("ERROR:  55P03: lock not available", "LockTimeout")]
        #[case(
            "ERROR:  40001: could not serialize access due to concurrent update",
            "LockTimeout"
        )]
        #[case("ERROR:  40P01: deadlock detected", "LockTimeout")]
        #[case(
            "ERROR:  57014: canceling statement due to statement timeout",
            "Timeout"
        )]
        #[case(
            "ERROR:  57014: canceling statement due to lock timeout",
            "LockTimeout"
        )]
        #[case("ERROR:  42P01: relation \"users\" does not exist", "ObjectMissing")]
        #[case("ERROR:  08006: connection to server was lost", "ConnectionLost")]
        #[case("ERROR:  08006: could not receive data from server", "ConnectionLost")]
        #[case(
            "FATAL:  57P01: terminating connection due to administrator command",
            "ConnectionLost"
        )]
        #[case(
            "FATAL:  57P02: terminating connection due to crash of another server",
            "ConnectionLost"
        )]
        #[case("ERROR:  57P03: the database system is starting up", "QueryFailed")]
        #[case("ERROR:  57P04: nearby unknown state", "QueryFailed")]
        #[case("ERROR:  08001: could not connect to server", "ConnectionFailed")]
        #[case("FATAL:  08001: could not translate host name", "HostUnreachable")]
        #[case("FATAL:  08001: timeout expired", "Timeout")]
        #[case("FATAL:  08001: connection refused", "ConnectionRefused")]
        #[case(
            "FATAL:  08001: server closed the connection unexpectedly",
            "ConnectionLost"
        )]
        #[case("FATAL:  28000: invalid authorization specification", "Auth")]
        #[case("FATAL:  28P01: opaque authentication failure", "Auth")]
        #[case("FATAL:  3D000: opaque database failure", "DatabaseNotFound")]
        fn classifies_sqlstate_first(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(classification_name(classify_query_error(input)), expected);
        }

        #[rstest]
        #[case(
            "ERROR:  25006: cannot execute in a read-only transaction",
            "Permission denied",
            "Check the connected user's privileges"
        )]
        #[case(
            "FATAL:  28000: invalid authorization specification",
            "Connection failed",
            "Check the connection settings and database availability"
        )]
        #[case(
            "ERROR:  40001: could not serialize access due to concurrent update",
            "Operation blocked by lock or timeout",
            "Retry; if it persists, check for blocking transactions or timeout settings"
        )]
        #[case(
            "ERROR:  40P01: deadlock detected",
            "Operation blocked by lock or timeout",
            "Retry; if it persists, check for blocking transactions or timeout settings"
        )]
        #[case(
            "FATAL:  57P01: terminating connection due to administrator command",
            "Connection lost during operation",
            "Reconnect and retry the operation"
        )]
        #[case(
            "FATAL:  57P02: terminating connection due to crash of another server",
            "Connection lost during operation",
            "Reconnect and retry the operation"
        )]
        #[case(
            "ERROR:  57P03: the database system is starting up",
            "Query failed",
            "Review the database error details and SQL"
        )]
        #[case(
            "ERROR:  57P04: nearby unknown state",
            "Query failed",
            "Review the database error details and SQL"
        )]
        fn known_sqlstates_preserve_details_and_presentation(
            #[case] input: &str,
            #[case] expected_summary: &str,
            #[case] expected_hint: &str,
        ) {
            let error = classify_query_error(input);

            assert_eq!(error.masked_details(), input);
            assert_eq!(error.summary(), expected_summary);
            assert_eq!(error.hint(), expected_hint);
        }

        #[rstest]
        #[case("ERROR: permission denied for table users", "PermissionDenied")]
        #[case(
            "ERROR: duplicate key value violates unique constraint",
            "UniqueViolation"
        )]
        #[case("ERROR: relation \"users\" does not exist", "ObjectMissing")]
        #[case("server closed the connection unexpectedly", "ConnectionLost")]
        #[case("ERROR: canceling statement due to statement timeout", "Timeout")]
        #[case(r#"FATAL: role "alice" does not exist"#, "DatabaseNotFound")]
        #[case(r#"ERROR: role "alice" does not exist"#, "QueryFailed")]
        #[case(r#"FATAL: password authentication failed for user "alice""#, "Auth")]
        #[case(
            r#"psql: error: could not translate host name "host" to address: Name or service not known"#,
            "HostUnreachable"
        )]
        #[case(
            r#"psql: error: connection to server at "host", port 5432 failed: Connection refused"#,
            "ConnectionRefused"
        )]
        fn falls_back_to_stderr_matching(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(classification_name(classify_query_error(input)), expected);
        }

        #[test]
        fn unknown_falls_back_safely() {
            assert!(matches!(
                classify_query_error("some random error"),
                DbOperationError::QueryFailed(_)
            ));
        }
    }
}
