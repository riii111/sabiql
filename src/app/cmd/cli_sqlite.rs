use std::io::ErrorKind;
use std::path::Path;

use crate::domain::{SqliteConnectionConfig, SqliteConnectionConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliSqliteDatabase {
    config: SqliteConnectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliSqliteDatabaseError {
    #[error("{0}")]
    Config(#[from] SqliteConnectionConfigError),
    #[error("Unsupported SQLite target; use a .db/.sqlite/.sqlite3 file path or sqlite:// DSN")]
    UnsupportedFormat,
    #[error("SQLite database file not found: {0}")]
    FileNotFound(String),
    #[error("Cannot access SQLite database file: {0}")]
    PathAccessDenied(String),
    #[error("Cannot read SQLite database file metadata: {0}")]
    Io(String),
    #[error("SQLite path is a directory, not a file: {0}")]
    IsDirectory(String),
}

impl CliSqliteDatabase {
    pub fn parse_cli_argument(input: &str) -> Result<Self, CliSqliteDatabaseError> {
        let path = parse_cli_path(input)?;
        Ok(Self {
            config: SqliteConnectionConfig::new(path)?,
        })
    }

    pub fn path(&self) -> &str {
        self.config.path()
    }

    pub fn dsn(&self) -> String {
        format!("sqlite://{}", self.config.path())
    }

    pub fn display_name(&self) -> String {
        Path::new(self.config.path())
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.config.path().to_string(), str::to_owned)
    }
}

pub fn resolve_cli_sqlite_database(
    input: &str,
) -> Result<CliSqliteDatabase, CliSqliteDatabaseError> {
    let database = CliSqliteDatabase::parse_cli_argument(input)?;
    validate_cli_sqlite_file(database.path())?;
    Ok(database)
}

fn validate_cli_sqlite_file(path: &str) -> Result<(), CliSqliteDatabaseError> {
    let path = Path::new(path);
    let display = path.display().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(metadata_error(&display, error.kind(), &error.to_string()));
        }
    };

    if metadata.is_dir() {
        return Err(CliSqliteDatabaseError::IsDirectory(display));
    }

    Ok(())
}

fn parse_cli_path(input: &str) -> Result<String, CliSqliteDatabaseError> {
    let trimmed = input.trim();
    let path = if let Some(path) = trimmed.strip_prefix("sqlite://") {
        if path.is_empty() {
            return Err(CliSqliteDatabaseError::UnsupportedFormat);
        }
        path
    } else {
        trimmed
    };

    validate_cli_path(path)
}

fn validate_cli_path(path: &str) -> Result<String, CliSqliteDatabaseError> {
    if looks_like_non_sqlite_target(path) {
        return Err(CliSqliteDatabaseError::UnsupportedFormat);
    }

    if !has_sqlite_file_extension(path) {
        return Err(CliSqliteDatabaseError::UnsupportedFormat);
    }

    Ok(path.to_string())
}

fn looks_like_non_sqlite_target(input: &str) -> bool {
    input.starts_with("service=") || input.contains("://")
}

fn has_sqlite_file_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("db")
                || ext.eq_ignore_ascii_case("sqlite")
                || ext.eq_ignore_ascii_case("sqlite3")
        })
}

fn metadata_error(display: &str, kind: ErrorKind, source: &str) -> CliSqliteDatabaseError {
    match kind {
        ErrorKind::NotFound => CliSqliteDatabaseError::FileNotFound(display.to_string()),
        ErrorKind::PermissionDenied => {
            CliSqliteDatabaseError::PathAccessDenied(format!("{display}: {source}"))
        }
        _ => CliSqliteDatabaseError::Io(format!("{display}: {source}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::fs;
    use tempfile::tempdir;

    mod parse_cli_argument {
        use super::*;

        #[test]
        fn accepts_sqlite_dsn() {
            let database = CliSqliteDatabase::parse_cli_argument("sqlite:///tmp/app.db").unwrap();

            assert_eq!(database.path(), "/tmp/app.db");
            assert_eq!(database.dsn(), "sqlite:///tmp/app.db");
        }

        #[rstest]
        #[case("app.db")]
        #[case("data.sqlite")]
        #[case("archive.SQLITE3")]
        #[case("./relative/app.db")]
        fn accepts_file_paths_with_supported_extensions(#[case] input: &str) {
            let database = CliSqliteDatabase::parse_cli_argument(input).unwrap();

            assert_eq!(database.path(), input);
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum ExpectedRejection {
            UnsupportedFormat,
            Config,
        }

        #[rstest]
        #[case("", ExpectedRejection::UnsupportedFormat)]
        #[case("   ", ExpectedRejection::UnsupportedFormat)]
        #[case("sqlite://", ExpectedRejection::UnsupportedFormat)]
        #[case("postgres://localhost/db", ExpectedRejection::UnsupportedFormat)]
        #[case("service=mydb", ExpectedRejection::UnsupportedFormat)]
        #[case("/tmp/app", ExpectedRejection::UnsupportedFormat)]
        #[case(":memory:", ExpectedRejection::UnsupportedFormat)]
        #[case("file:/tmp/app.db", ExpectedRejection::Config)]
        #[case("sqlite:///tmp/app", ExpectedRejection::UnsupportedFormat)]
        #[case("sqlite://:memory:", ExpectedRejection::UnsupportedFormat)]
        fn rejects_unsupported_targets(#[case] input: &str, #[case] expected: ExpectedRejection) {
            let result = CliSqliteDatabase::parse_cli_argument(input);

            match expected {
                ExpectedRejection::UnsupportedFormat => {
                    assert!(matches!(
                        result,
                        Err(CliSqliteDatabaseError::UnsupportedFormat)
                    ));
                }
                ExpectedRejection::Config => {
                    assert!(matches!(result, Err(CliSqliteDatabaseError::Config(_))));
                }
            }
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn uses_file_name() {
            let database = CliSqliteDatabase::parse_cli_argument("/tmp/projects/app.db").unwrap();

            assert_eq!(database.display_name(), "app.db");
        }
    }

    mod metadata_error {
        use super::*;

        #[rstest]
        #[case(
            ErrorKind::NotFound,
            "No such file",
            CliSqliteDatabaseError::FileNotFound("/tmp/app.db".to_string())
        )]
        #[case(
            ErrorKind::PermissionDenied,
            "permission denied",
            CliSqliteDatabaseError::PathAccessDenied("/tmp/app.db: permission denied".to_string())
        )]
        #[case(
            ErrorKind::Other,
            "device offline",
            CliSqliteDatabaseError::Io("/tmp/app.db: device offline".to_string())
        )]
        fn maps_error_kind_to_cli_error(
            #[case] kind: ErrorKind,
            #[case] source: &str,
            #[case] expected: CliSqliteDatabaseError,
        ) {
            assert_eq!(metadata_error("/tmp/app.db", kind, source), expected);
        }
    }

    mod validate_cli_sqlite_file {
        use super::*;

        #[test]
        fn accepts_existing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();

            assert!(validate_cli_sqlite_file(path.to_str().unwrap()).is_ok());
        }

        #[test]
        fn rejects_missing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");

            assert!(matches!(
                validate_cli_sqlite_file(path.to_str().unwrap()),
                Err(CliSqliteDatabaseError::FileNotFound(_))
            ));
        }

        #[test]
        fn rejects_directory() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("folder.db");
            fs::create_dir(&path).unwrap();

            assert!(matches!(
                validate_cli_sqlite_file(path.to_str().unwrap()),
                Err(CliSqliteDatabaseError::IsDirectory(_))
            ));
        }
    }

    mod resolve_cli_sqlite_database {
        use super::*;

        #[test]
        fn resolves_existing_sqlite_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();

            let database = resolve_cli_sqlite_database(path.to_str().unwrap()).unwrap();

            assert_eq!(database.path(), path.to_str().unwrap());
            assert_eq!(database.dsn(), format!("sqlite://{}", path.display()));
        }

        #[test]
        fn rejects_missing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");

            let error = resolve_cli_sqlite_database(path.to_str().unwrap()).unwrap_err();

            assert!(matches!(error, CliSqliteDatabaseError::FileNotFound(_)));
        }
    }
}
