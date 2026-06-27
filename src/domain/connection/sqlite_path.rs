#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SqlitePathError {
    #[error("SQLite database file not found: {0}")]
    FileNotFound(String),
    #[error("SQLite path is a directory, not a file: {0}")]
    IsDirectory(String),
    #[error("SQLite path is not a regular file: {0}")]
    NotRegularFile(String),
    #[error("Cannot read SQLite database file: {0}")]
    ReadAccessDenied(String),
    #[error("Cannot access SQLite database file: {0}")]
    PathAccessDenied(String),
    #[error("Cannot read SQLite database file metadata: {0}")]
    Io(String),
}

pub fn sqlite_path_from_dsn(dsn: &str) -> Option<&str> {
    dsn.strip_prefix("sqlite://")
        .filter(|path| !path.is_empty())
}

pub fn classify_sqlite_metadata_error(
    display: &str,
    kind: std::io::ErrorKind,
    source: &str,
) -> SqlitePathError {
    match kind {
        std::io::ErrorKind::NotFound => SqlitePathError::FileNotFound(display.to_string()),
        std::io::ErrorKind::PermissionDenied => {
            SqlitePathError::PathAccessDenied(format!("{display}: {source}"))
        }
        _ => SqlitePathError::Io(format!("{display}: {source}")),
    }
}

pub fn classify_sqlite_read_error(
    display: &str,
    kind: std::io::ErrorKind,
    source: &str,
) -> SqlitePathError {
    match kind {
        std::io::ErrorKind::PermissionDenied => {
            SqlitePathError::ReadAccessDenied(format!("{display}: {source}"))
        }
        std::io::ErrorKind::NotFound => SqlitePathError::FileNotFound(display.to_string()),
        _ => SqlitePathError::Io(format!("{display}: {source}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod sqlite_path_from_dsn {
        use super::*;

        #[test]
        fn extracts_path_from_dsn() {
            assert_eq!(
                sqlite_path_from_dsn("sqlite:///tmp/app.db"),
                Some("/tmp/app.db")
            );
        }

        #[rstest]
        #[case("postgres://localhost/db")]
        #[case("sqlite://")]
        #[case("")]
        fn returns_none_for_non_sqlite_dsn(#[case] dsn: &str) {
            assert_eq!(sqlite_path_from_dsn(dsn), None);
        }
    }

    mod classify_errors {
        use super::*;

        #[test]
        fn metadata_not_found() {
            assert_eq!(
                classify_sqlite_metadata_error(
                    "/tmp/app.db",
                    std::io::ErrorKind::NotFound,
                    "missing"
                ),
                SqlitePathError::FileNotFound("/tmp/app.db".to_string())
            );
        }

        #[test]
        fn read_permission_denied() {
            assert_eq!(
                classify_sqlite_read_error(
                    "/tmp/app.db",
                    std::io::ErrorKind::PermissionDenied,
                    "permission denied"
                ),
                SqlitePathError::ReadAccessDenied("/tmp/app.db: permission denied".to_string())
            );
        }
    }
}
