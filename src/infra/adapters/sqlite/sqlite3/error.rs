use crate::app::ports::outbound::DbOperationError;

pub(in crate::adapters::sqlite) fn classify_cli_spawn_error(
    error: std::io::Error,
) -> DbOperationError {
    if error.kind() == std::io::ErrorKind::NotFound {
        DbOperationError::CommandNotFound(format!("sqlite3: {error}"))
    } else {
        DbOperationError::QueryFailed(error.to_string())
    }
}

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
        || lower.contains("no such index")
        || lower.contains("no such trigger")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ClassifiedKind {
        PermissionDenied,
        ForeignKeyViolation,
        UniqueViolation,
        LockTimeout,
        ObjectMissing,
        QueryFailed,
        Other,
    }

    fn classified_kind(error: &DbOperationError) -> ClassifiedKind {
        match error {
            DbOperationError::PermissionDenied(_) => ClassifiedKind::PermissionDenied,
            DbOperationError::ForeignKeyViolation(_) => ClassifiedKind::ForeignKeyViolation,
            DbOperationError::UniqueViolation(_) => ClassifiedKind::UniqueViolation,
            DbOperationError::LockTimeout(_) => ClassifiedKind::LockTimeout,
            DbOperationError::ObjectMissing(_) => ClassifiedKind::ObjectMissing,
            DbOperationError::QueryFailed(_) => ClassifiedKind::QueryFailed,
            _ => ClassifiedKind::Other,
        }
    }

    mod classification {
        use super::*;

        #[rstest]
        #[case("Error: database is locked", ClassifiedKind::LockTimeout)]
        #[case(
            "Runtime error: database is locked (SQLITE_BUSY)",
            ClassifiedKind::LockTimeout
        )]
        #[case(
            "Error: attempt to write a readonly database",
            ClassifiedKind::PermissionDenied
        )]
        #[case(
            "Error: FOREIGN KEY constraint failed",
            ClassifiedKind::ForeignKeyViolation
        )]
        #[case(
            "Error: UNIQUE constraint failed: users.email",
            ClassifiedKind::UniqueViolation
        )]
        #[case(
            "Parse error: near \"SELEKT\": syntax error",
            ClassifiedKind::QueryFailed
        )]
        #[case("Error: near \"SELEKT\": syntax error", ClassifiedKind::QueryFailed)]
        #[case("Error: no such table: users", ClassifiedKind::ObjectMissing)]
        #[case("Error: no such column: missing", ClassifiedKind::ObjectMissing)]
        #[case("Error: no such index: missing_idx", ClassifiedKind::ObjectMissing)]
        #[case(
            "Error: no such trigger: missing_trigger",
            ClassifiedKind::ObjectMissing
        )]
        #[case(
            "Error: in prepare, no such table: main.pragma_table_list",
            ClassifiedKind::QueryFailed
        )]
        fn classifies_sqlite_stderr(#[case] input: &str, #[case] expected: ClassifiedKind) {
            let error = classify_query_error(input);

            assert_eq!(classified_kind(&error), expected);
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
            let error = classify_query_error("some random error");

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }

        #[test]
        fn empty_stderr_falls_back_to_query_failed() {
            let error = classify_query_error("   ");

            assert!(matches!(
                error,
                DbOperationError::QueryFailed(details) if details.is_empty()
            ));
        }
    }

    #[test]
    fn missing_sqlite_cli_has_command_specific_details() {
        let error = classify_cli_spawn_error(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No such file or directory",
        ));

        assert!(matches!(
            error,
            DbOperationError::CommandNotFound(details) if details.starts_with("sqlite3:")
        ));
    }
}
