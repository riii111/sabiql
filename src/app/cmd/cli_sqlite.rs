use std::path::Path;

use uuid::{Uuid, uuid};

use crate::domain::{
    ConnectionId, SqliteConnectionConfig, SqliteConnectionConfigError, SqlitePathError,
};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::SqlitePathValidator;

const CLI_SQLITE_CONNECTION_NAMESPACE: Uuid = uuid!("a3b5c7d9-1e2f-4a6b-8c0d-2e4f6a8b0c1d");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliSqliteTarget {
    config: SqliteConnectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliSqliteTargetError {
    #[error("{0}")]
    Config(#[from] SqliteConnectionConfigError),
    #[error("Unsupported SQLite target; use a SQLite database file path or sqlite:// DSN")]
    UnsupportedFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliSqliteResolveError {
    #[error("{0}")]
    Target(#[from] CliSqliteTargetError),
    #[error("{0}")]
    Path(#[from] SqlitePathError),
    #[error("invalid SQLite path")]
    InvalidPathEncoding,
}

#[derive(Debug, thiserror::Error)]
pub enum CliSqliteActivateError {
    #[error("Cannot resolve SQLite database path: {0}")]
    Path(#[from] SqlitePathError),
    #[error("Cannot represent the canonical SQLite database path")]
    InvalidPathEncoding,
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

    pub fn path_for_validation(&self) -> &Path {
        Path::new(self.config.path())
    }
}

pub fn connection_id_for_path(path: &str) -> ConnectionId {
    let derived = Uuid::new_v5(&CLI_SQLITE_CONNECTION_NAMESPACE, path.as_bytes());
    ConnectionId::from_string(format!("cli-sqlite-{}", derived.as_simple()))
}

pub fn resolve_cli_sqlite_target(
    database: &str,
    validator: &impl SqlitePathValidator,
) -> Result<CliSqliteTarget, CliSqliteResolveError> {
    let target = CliSqliteTarget::parse_cli_argument(database)?;
    let path = target
        .path_for_validation()
        .to_str()
        .ok_or(CliSqliteResolveError::InvalidPathEncoding)?;
    validator.validate_database_path(path)?;
    Ok(target)
}

pub fn activate_cli_sqlite_connection(
    state: &mut AppState,
    target: &CliSqliteTarget,
    validator: &impl SqlitePathValidator,
) -> Result<(), CliSqliteActivateError> {
    let canonical_path = validator.canonicalize_database_path(target.path())?;
    let canonical_path = canonical_path
        .to_str()
        .ok_or(CliSqliteActivateError::InvalidPathEncoding)?;
    let connection_id = connection_id_for_path(canonical_path);
    state.session.activate_cli_ephemeral_connection(
        &connection_id,
        &target.display_name(),
        &format!("sqlite://{canonical_path}"),
    );
    state.modal.set_mode(InputMode::Normal);
    Ok(())
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
    if path.is_empty() {
        return Err(CliSqliteTargetError::UnsupportedFormat);
    }

    if looks_like_non_sqlite_target(path) {
        return Err(CliSqliteTargetError::UnsupportedFormat);
    }

    Ok(path.to_string())
}

fn looks_like_non_sqlite_target(input: &str) -> bool {
    input.starts_with("service=") || input.contains("://")
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
        #[case("History")]
        #[case("data.sqlite")]
        #[case("archive.SQLITE3")]
        #[case("./relative/app.db")]
        fn accepts_file_paths_without_extension_filtering(#[case] input: &str) {
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
        #[case(":memory:", ExpectedRejection::Config)]
        #[case("file:/tmp/app.db", ExpectedRejection::Config)]
        #[case("sqlite://:memory:", ExpectedRejection::Config)]
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

    mod connection_id_for_path {
        use super::*;

        #[test]
        fn is_stable_for_same_path() {
            let first = connection_id_for_path("/tmp/app.db");
            let second = connection_id_for_path("/tmp/app.db");

            assert_eq!(first, second);
        }

        #[test]
        fn is_file_name_safe() {
            let connection_id = connection_id_for_path("/tmp/app.db");

            assert!(connection_id.as_str().starts_with("cli-sqlite-"));
            assert!(!connection_id.as_str().contains('/'));
        }

        #[test]
        fn differs_for_different_paths() {
            let first = connection_id_for_path("/tmp/app.db");
            let second = connection_id_for_path("/tmp/other.db");

            assert_ne!(first, second);
        }
    }
}
