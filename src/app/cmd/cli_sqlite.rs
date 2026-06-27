use std::io::ErrorKind;
use std::path::Path;

use crate::domain::{ConnectionId, SqliteConnectionConfig, SqliteConnectionConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliSqliteTarget {
    config: SqliteConnectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliSqliteTargetError {
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

impl CliSqliteTargetError {
    pub fn from_file_metadata_error(path_display: &str, error: &std::io::Error) -> Self {
        match error.kind() {
            ErrorKind::NotFound => Self::FileNotFound(path_display.to_string()),
            ErrorKind::PermissionDenied => {
                Self::PathAccessDenied(format!("{path_display}: {error}"))
            }
            _ => Self::Io(format!("{path_display}: {error}")),
        }
    }
}

impl CliSqliteTarget {
    pub fn parse_cli_argument(input: &str) -> Result<Self, CliSqliteTargetError> {
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

pub fn connection_id_for_path(path: &str) -> ConnectionId {
    ConnectionId::from_string(format!("cli-sqlite:{path}"))
}

fn parse_cli_path(input: &str) -> Result<String, CliSqliteTargetError> {
    let trimmed = input.trim();
    let path = if let Some(path) = trimmed.strip_prefix("sqlite://") {
        if path.is_empty() {
            return Err(CliSqliteTargetError::UnsupportedFormat);
        }
        path
    } else {
        trimmed
    };

    validate_cli_path(path)
}

fn validate_cli_path(path: &str) -> Result<String, CliSqliteTargetError> {
    if looks_like_non_sqlite_target(path) {
        return Err(CliSqliteTargetError::UnsupportedFormat);
    }

    if !has_sqlite_file_extension(path) {
        return Err(CliSqliteTargetError::UnsupportedFormat);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod parse_cli_argument {
        use super::*;

        #[test]
        fn accepts_sqlite_dsn() {
            let target = CliSqliteTarget::parse_cli_argument("sqlite:///tmp/app.db").unwrap();

            assert_eq!(target.path(), "/tmp/app.db");
            assert_eq!(target.dsn(), "sqlite:///tmp/app.db");
        }

        #[rstest]
        #[case("app.db")]
        #[case("data.sqlite")]
        #[case("archive.SQLITE3")]
        #[case("./relative/app.db")]
        fn accepts_file_paths_with_supported_extensions(#[case] input: &str) {
            let target = CliSqliteTarget::parse_cli_argument(input).unwrap();

            assert_eq!(target.path(), input);
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
            let result = CliSqliteTarget::parse_cli_argument(input);

            match expected {
                ExpectedRejection::UnsupportedFormat => {
                    assert!(matches!(
                        result,
                        Err(CliSqliteTargetError::UnsupportedFormat)
                    ));
                }
                ExpectedRejection::Config => {
                    assert!(matches!(result, Err(CliSqliteTargetError::Config(_))));
                }
            }
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn uses_file_name() {
            let target = CliSqliteTarget::parse_cli_argument("/tmp/projects/app.db").unwrap();

            assert_eq!(target.display_name(), "app.db");
        }
    }

    mod from_file_metadata_error {
        use super::*;
        use std::io::{Error, ErrorKind};

        #[rstest]
        #[case(
            ErrorKind::NotFound,
            "No such file",
            CliSqliteTargetError::FileNotFound("/tmp/app.db".to_string())
        )]
        #[case(
            ErrorKind::PermissionDenied,
            "permission denied",
            CliSqliteTargetError::PathAccessDenied("/tmp/app.db: permission denied".to_string())
        )]
        #[case(
            ErrorKind::Other,
            "device offline",
            CliSqliteTargetError::Io("/tmp/app.db: device offline".to_string())
        )]
        fn maps_error_kind(
            #[case] kind: ErrorKind,
            #[case] message: &str,
            #[case] expected: CliSqliteTargetError,
        ) {
            let error = Error::new(kind, message);
            assert_eq!(
                CliSqliteTargetError::from_file_metadata_error("/tmp/app.db", &error),
                expected
            );
        }
    }

    mod connection_id_for_path {
        use super::*;

        #[test]
        fn is_stable_for_same_path() {
            let first = connection_id_for_path("/tmp/app.db");
            let second = connection_id_for_path("/tmp/app.db");

            assert_eq!(first, second);
            assert_eq!(first.as_str(), "cli-sqlite:/tmp/app.db");
        }
    }
}
