use crate::app::ports::outbound::DbOperationError;

pub(in crate::adapters::sqlite) fn classify_query_error(stderr: &str) -> DbOperationError {
    let trimmed = stderr.trim();
    let Some(details) = (!trimmed.is_empty()).then_some(trimmed) else {
        return DbOperationError::QueryFailed(String::new());
    };

    classify_by_stderr(details)
}

fn classify_by_stderr(details: &str) -> DbOperationError {
    let lower = details.to_ascii_lowercase();

    // Keep table-list fallback in SqliteAdapter::list_tables working.
    if lower.contains("pragma_table_list") {
        return DbOperationError::QueryFailed(details.to_string());
    }

    if is_locked(&lower) {
        return DbOperationError::LockTimeout(details.to_string());
    }

    if is_readonly(&lower) {
        return DbOperationError::PermissionDenied(details.to_string());
    }

    if lower.contains("foreign key constraint failed") {
        return DbOperationError::ForeignKeyViolation(details.to_string());
    }

    if lower.contains("unique constraint failed") {
        return DbOperationError::UniqueViolation(details.to_string());
    }

    if is_missing_object(&lower) {
        return DbOperationError::ObjectMissing(details.to_string());
    }

    DbOperationError::QueryFailed(details.to_string())
}

fn is_locked(lower: &str) -> bool {
    lower.contains("database is locked")
        || lower.contains("database table is locked")
        || lower.contains("sqlite_busy")
}

fn is_readonly(lower: &str) -> bool {
    lower.contains("readonly database") || lower.contains("read-only database")
}

fn is_missing_object(lower: &str) -> bool {
    lower.contains("no such table")
        || lower.contains("no such column")
        || lower.contains("no such view")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod classification {
        use super::*;

        #[rstest]
        #[case("Error: database is locked", "LockTimeout")]
        #[case("Runtime error: database is locked (SQLITE_BUSY)", "LockTimeout")]
        #[case("Error: attempt to write a readonly database", "PermissionDenied")]
        #[case("Error: FOREIGN KEY constraint failed", "ForeignKeyViolation")]
        #[case("Error: UNIQUE constraint failed: users.email", "UniqueViolation")]
        #[case("Parse error: near \"SELEKT\": syntax error", "QueryFailed")]
        #[case("Error: near \"SELEKT\": syntax error", "QueryFailed")]
        #[case("Error: no such table: users", "ObjectMissing")]
        #[case("Error: no such column: missing", "ObjectMissing")]
        #[case(
            "Error: in prepare, no such table: main.pragma_table_list",
            "QueryFailed"
        )]
        fn classifies_sqlite_stderr(#[case] input: &str, #[case] expected: &str) {
            let error = classify_query_error(input);
            let actual = match error {
                DbOperationError::PermissionDenied(_) => "PermissionDenied",
                DbOperationError::ForeignKeyViolation(_) => "ForeignKeyViolation",
                DbOperationError::UniqueViolation(_) => "UniqueViolation",
                DbOperationError::LockTimeout(_) => "LockTimeout",
                DbOperationError::ObjectMissing(_) => "ObjectMissing",
                DbOperationError::QueryFailed(_) => "QueryFailed",
                _ => "Other",
            };

            assert_eq!(actual, expected);
        }

        #[test]
        fn lock_readonly_and_constraints_use_distinct_summaries() {
            let lock = classify_query_error("Error: database is locked");
            let readonly = classify_query_error("Error: attempt to write a readonly database");
            let foreign_key = classify_query_error("Error: FOREIGN KEY constraint failed");
            let unique = classify_query_error("Error: UNIQUE constraint failed: users.email");

            assert_ne!(lock.summary(), readonly.summary());
            assert_ne!(lock.summary(), foreign_key.summary());
            assert_ne!(lock.summary(), unique.summary());
            assert_ne!(readonly.summary(), foreign_key.summary());
            assert_ne!(foreign_key.summary(), unique.summary());
        }

        #[test]
        fn unknown_falls_back_safely() {
            assert!(matches!(
                classify_query_error("some random error"),
                DbOperationError::QueryFailed(_)
            ));
        }

        #[test]
        fn empty_stderr_falls_back_to_query_failed() {
            assert!(matches!(
                classify_query_error("   "),
                DbOperationError::QueryFailed(details) if details.is_empty()
            ));
        }
    }
}
