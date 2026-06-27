use std::path::Path;

use super::config::{SqliteConnectionConfig, SqliteConnectionConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteStartupTarget {
    config: SqliteConnectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SqliteStartupError {
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

impl SqliteStartupTarget {
    pub fn from_cli_input(input: &str) -> Result<Self, SqliteStartupError> {
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

    pub fn validate_file_on_disk(&self) -> Result<(), SqliteStartupError> {
        validate_sqlite_file_path(Path::new(self.config.path()))
    }
}

fn parse_cli_path(input: &str) -> Result<String, SqliteStartupError> {
    let trimmed = input.trim();
    if let Some(path) = trimmed.strip_prefix("sqlite://") {
        if path.is_empty() {
            return Err(SqliteStartupError::UnsupportedFormat);
        }
        return Ok(path.to_string());
    }

    if looks_like_non_sqlite_target(trimmed) {
        return Err(SqliteStartupError::UnsupportedFormat);
    }

    if !has_sqlite_file_extension(trimmed) {
        return Err(SqliteStartupError::UnsupportedFormat);
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

fn validate_sqlite_file_path(path: &Path) -> Result<(), SqliteStartupError> {
    let display = path.display().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(SqliteStartupError::FileNotFound(display));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(SqliteStartupError::PathAccessDenied(format!(
                "{display}: {error}"
            )));
        }
        Err(error) => {
            return Err(SqliteStartupError::Io(format!("{display}: {error}")));
        }
    };

    if metadata.is_dir() {
        return Err(SqliteStartupError::IsDirectory(display));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::fs;
    use tempfile::tempdir;

    mod parse_cli_path {
        use super::*;

        #[test]
        fn accepts_sqlite_dsn() {
            let target = SqliteStartupTarget::from_cli_input("sqlite:///tmp/app.db").unwrap();

            assert_eq!(target.path(), "/tmp/app.db");
            assert_eq!(target.dsn(), "sqlite:///tmp/app.db");
        }

        #[rstest]
        #[case("app.db")]
        #[case("data.sqlite")]
        #[case("archive.SQLITE3")]
        #[case("./relative/app.db")]
        fn accepts_file_paths_with_supported_extensions(#[case] input: &str) {
            let target = SqliteStartupTarget::from_cli_input(input).unwrap();

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
                SqliteStartupTarget::from_cli_input(input),
                Err(SqliteStartupError::UnsupportedFormat | SqliteStartupError::Config(_))
            ));
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn uses_file_name() {
            let target = SqliteStartupTarget::from_cli_input("/tmp/projects/app.db").unwrap();

            assert_eq!(target.display_name(), "app.db");
        }
    }

    mod validate_file_on_disk {
        use super::*;

        #[test]
        fn accepts_existing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();
            let target = SqliteStartupTarget::from_cli_input(path.to_str().unwrap()).unwrap();

            assert!(target.validate_file_on_disk().is_ok());
        }

        #[test]
        fn rejects_missing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let target = SqliteStartupTarget::from_cli_input(path.to_str().unwrap()).unwrap();

            assert!(matches!(
                target.validate_file_on_disk(),
                Err(SqliteStartupError::FileNotFound(_))
            ));
        }

        #[test]
        fn rejects_directory() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("folder.db");
            fs::create_dir(&path).unwrap();
            let target = SqliteStartupTarget::from_cli_input(path.to_str().unwrap()).unwrap();

            assert!(matches!(
                target.validate_file_on_disk(),
                Err(SqliteStartupError::IsDirectory(_))
            ));
        }
    }
}
