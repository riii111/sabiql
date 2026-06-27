use std::path::Path;

use crate::domain::connection::{SqliteConnectionConfig, SqliteConnectionConfigError};
use crate::domain::{ConnectionId, DatabaseType, SqlitePathError};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::SqlitePathValidator;

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

impl CliSqliteTarget {
    pub fn from_cli_input(input: &str) -> Result<Self, CliSqliteTargetError> {
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

pub fn resolve_cli_sqlite_target(
    database: &str,
    validator: &impl SqlitePathValidator,
) -> Result<CliSqliteTarget, CliSqliteResolveError> {
    let target = CliSqliteTarget::from_cli_input(database)?;
    let path = target
        .path_for_validation()
        .to_str()
        .ok_or(CliSqliteResolveError::InvalidPathEncoding)?;
    validator.validate_database_path(path)?;
    Ok(target)
}

pub fn activate_cli_sqlite_connection(state: &mut AppState, target: &CliSqliteTarget) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::ephemeral_cli(),
        &target.display_name(),
        DatabaseType::SQLite,
        &target.dsn(),
    );
    state.modal.set_mode(InputMode::Normal);
}

fn parse_cli_path(input: &str) -> Result<String, CliSqliteTargetError> {
    let trimmed = input.trim();
    if let Some(path) = trimmed.strip_prefix("sqlite://") {
        if path.is_empty() {
            return Err(CliSqliteTargetError::UnsupportedFormat);
        }
        return Ok(path.to_string());
    }

    if looks_like_non_sqlite_target(trimmed) {
        return Err(CliSqliteTargetError::UnsupportedFormat);
    }

    if !has_sqlite_file_extension(trimmed) {
        return Err(CliSqliteTargetError::UnsupportedFormat);
    }

    Ok(trimmed.to_string())
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

    mod parse_cli_path {
        use super::*;

        #[test]
        fn accepts_sqlite_dsn() {
            let target = CliSqliteTarget::from_cli_input("sqlite:///tmp/app.db").unwrap();

            assert_eq!(target.path(), "/tmp/app.db");
            assert_eq!(target.dsn(), "sqlite:///tmp/app.db");
        }

        #[rstest]
        #[case("app.db")]
        #[case("data.sqlite")]
        #[case("archive.SQLITE3")]
        #[case("./relative/app.db")]
        fn accepts_file_paths_with_supported_extensions(#[case] input: &str) {
            let target = CliSqliteTarget::from_cli_input(input).unwrap();

            assert_eq!(target.path(), input);
        }

        #[rstest]
        #[case("")]
        #[case("   ")]
        #[case("sqlite://")]
        #[case("postgres://localhost/db")]
        #[case("service=mydb")]
        #[case("/tmp/app")]
        #[case(":memory:")]
        #[case("file:/tmp/app.db")]
        fn rejects_unsupported_targets(#[case] input: &str) {
            assert!(matches!(
                CliSqliteTarget::from_cli_input(input),
                Err(CliSqliteTargetError::UnsupportedFormat | CliSqliteTargetError::Config(_))
            ));
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn uses_file_name() {
            let target = CliSqliteTarget::from_cli_input("/tmp/projects/app.db").unwrap();

            assert_eq!(target.display_name(), "app.db");
        }
    }
}
