use crate::app::ports::DbOperationError;

pub(in crate::infra::adapters::postgres) fn classify_query_error(stderr: &str) -> DbOperationError {
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
    if sqlstate.starts_with("08") {
        return if is_connection_lost_message(&details.to_lowercase()) {
            DbOperationError::ConnectionLost(details.to_string())
        } else {
            DbOperationError::ConnectionFailed(details.to_string())
        };
    }

    match sqlstate {
        "28P01" | "3D000" => DbOperationError::ConnectionFailed(details.to_string()),
        "42501" => DbOperationError::PermissionDenied(details.to_string()),
        "23503" => DbOperationError::ForeignKeyViolation(details.to_string()),
        "23505" => DbOperationError::UniqueViolation(details.to_string()),
        "55P03" => DbOperationError::LockTimeout(details.to_string()),
        "57014" => classify_query_canceled(details),
        "42P01" | "42703" => DbOperationError::ObjectMissing(details.to_string()),
        _ => DbOperationError::QueryFailed(details.to_string()),
    }
}

fn classify_by_stderr(details: &str) -> DbOperationError {
    let lower = details.to_lowercase();

    if lower.contains("permission denied") || lower.contains("must be owner of") {
        return DbOperationError::PermissionDenied(details.to_string());
    }

    if lower.contains("password authentication failed")
        || lower.contains("authentication failed")
        || lower.contains("could not translate host name")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname provided")
        || is_missing_database_or_role(&lower)
        || lower.contains("connection refused")
        || lower.contains("could not connect to server")
    {
        return DbOperationError::ConnectionFailed(details.to_string());
    }

    if lower.contains("violates foreign key constraint")
        || lower.contains("foreign key constraint")
    {
        return DbOperationError::ForeignKeyViolation(details.to_string());
    }

    if lower.contains("duplicate key value")
        || lower.contains("violates unique constraint")
        || lower.contains("unique constraint")
    {
        return DbOperationError::UniqueViolation(details.to_string());
    }

    if lower.contains("lock not available") || lower.contains("canceling statement due to lock timeout")
    {
        return DbOperationError::LockTimeout(details.to_string());
    }

    if lower.contains("canceling statement due to statement timeout")
        || lower.contains("statement timeout")
        || lower.contains("query canceled")
        || lower.contains("canceling statement due to user request")
    {
        return DbOperationError::Timeout(details.to_string());
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
    lower.contains("does not exist")
        && (lower.contains("database") || lower.contains("role"))
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
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.trim_start();
        if rest.len() < 6 {
            continue;
        }
        let code = &rest[..5];
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
    code.len() == 5 && code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod sqlstate {
        use super::*;

        #[rstest]
        #[case("ERROR:  42501: permission denied for table users", "42501")]
        #[case("FATAL:  23505: duplicate key value violates unique constraint", "23505")]
        #[case("SQL state: 42P01", "42P01")]
        fn extracts_codes(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(extract_sqlstate(input), Some(expected));
        }
    }

    mod classification {
        use super::*;

        #[rstest]
        #[case(
            "ERROR:  42501: permission denied for table users",
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
        #[case("ERROR:  57014: canceling statement due to statement timeout", "Timeout")]
        #[case("ERROR:  42P01: relation \"users\" does not exist", "ObjectMissing")]
        #[case("ERROR:  08006: connection to server was lost", "ConnectionLost")]
        fn classifies_sqlstate_first(#[case] input: &str, #[case] expected: &str) {
            let error = classify_query_error(input);
            let actual = match error {
                DbOperationError::PermissionDenied(_) => "PermissionDenied",
                DbOperationError::ForeignKeyViolation(_) => "ForeignKeyViolation",
                DbOperationError::UniqueViolation(_) => "UniqueViolation",
                DbOperationError::LockTimeout(_) => "LockTimeout",
                DbOperationError::Timeout(_) => "Timeout",
                DbOperationError::ObjectMissing(_) => "ObjectMissing",
                DbOperationError::ConnectionLost(_) => "ConnectionLost",
                _ => "Other",
            };

            assert_eq!(actual, expected);
        }

        #[rstest]
        #[case(
            "ERROR: permission denied for table users",
            "PermissionDenied"
        )]
        #[case(
            "ERROR: duplicate key value violates unique constraint",
            "UniqueViolation"
        )]
        #[case(
            "ERROR: relation \"users\" does not exist",
            "ObjectMissing"
        )]
        #[case(
            "server closed the connection unexpectedly",
            "ConnectionLost"
        )]
        #[case(
            "ERROR: canceling statement due to statement timeout",
            "Timeout"
        )]
        #[case(r#"FATAL: role "alice" does not exist"#, "ConnectionFailed")]
        fn falls_back_to_stderr_matching(#[case] input: &str, #[case] expected: &str) {
            let error = classify_query_error(input);
            let actual = match error {
                DbOperationError::PermissionDenied(_) => "PermissionDenied",
                DbOperationError::UniqueViolation(_) => "UniqueViolation",
                DbOperationError::ObjectMissing(_) => "ObjectMissing",
                DbOperationError::ConnectionLost(_) => "ConnectionLost",
                DbOperationError::Timeout(_) => "Timeout",
                DbOperationError::ConnectionFailed(_) => "ConnectionFailed",
                _ => "Other",
            };

            assert_eq!(actual, expected);
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
