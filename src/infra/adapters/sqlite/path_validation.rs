use std::path::Path;

use crate::app::ports::outbound::SqlitePathValidator;
use crate::domain::{SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error};

#[derive(Debug, Default, Clone, Copy)]
pub struct FsSqlitePathValidator;

impl SqlitePathValidator for FsSqlitePathValidator {
    fn validate_database_path(&self, path: &str) -> Result<(), SqlitePathError> {
        validate_sqlite_database_path(Path::new(path))
    }
}

fn validate_sqlite_database_path(path: &Path) -> Result<(), SqlitePathError> {
    let display = path.display().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(classify_sqlite_metadata_error(
                &display,
                error.kind(),
                &error.to_string(),
            ));
        }
    };

    if metadata.is_dir() {
        return Err(SqlitePathError::IsDirectory(display));
    }

    if !metadata.is_file() {
        return Err(SqlitePathError::NotRegularFile(display));
    }

    match std::fs::File::open(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(classify_sqlite_read_error(
            &display,
            error.kind(),
            &error.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        fs::write(&path, b"").unwrap();

        assert!(
            FsSqlitePathValidator
                .validate_database_path(path.to_str().unwrap())
                .is_ok()
        );
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.db");

        assert!(matches!(
            FsSqlitePathValidator.validate_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::FileNotFound(_))
        ));
    }

    #[test]
    fn rejects_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folder.db");
        fs::create_dir(&path).unwrap();

        assert!(matches!(
            FsSqlitePathValidator.validate_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::IsDirectory(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_fifo_without_opening() {
        use std::process::Command;

        let dir = tempdir().unwrap();
        let path = dir.path().join("pipe.db");
        Command::new("mkfifo").arg(&path).status().expect("mkfifo");

        assert!(matches!(
            FsSqlitePathValidator.validate_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::NotRegularFile(_))
        ));
    }
}
