use std::path::{Path, PathBuf};

use crate::app::ports::outbound::SqlitePathValidator;
use crate::domain::{SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error};

const SQLITE_HEADER_MAGIC: &[u8; 16] = b"SQLite format 3\0";

#[derive(Debug, Default, Clone, Copy)]
pub struct FsSqlitePathValidator;

impl SqlitePathValidator for FsSqlitePathValidator {
    fn validate_database_path(&self, path: &str) -> Result<(), SqlitePathError> {
        validate_sqlite_database_path(Path::new(path))
    }

    fn canonicalize_database_path(&self, path: &str) -> Result<PathBuf, SqlitePathError> {
        let path = Path::new(path);
        let display = path.display().to_string();
        std::fs::canonicalize(path).map_err(|error| {
            classify_sqlite_metadata_error(&display, error.kind(), &error.to_string())
        })
    }
}

pub(super) fn validate_sqlite_database_path(path: &Path) -> Result<(), SqlitePathError> {
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
        Ok(mut file) => validate_sqlite_header(&mut file, metadata.len(), display),
        Err(error) => Err(classify_sqlite_read_error(
            &display,
            error.kind(),
            &error.to_string(),
        )),
    }
}

fn validate_sqlite_header(
    file: &mut std::fs::File,
    file_len: u64,
    display: String,
) -> Result<(), SqlitePathError> {
    use std::io::Read;

    if file_len == 0 {
        return Ok(());
    }

    let mut header = [0; 16];
    match file.read_exact(&mut header) {
        Ok(()) if &header == SQLITE_HEADER_MAGIC => Ok(()),
        Ok(()) => Err(SqlitePathError::NotDatabaseFile(display)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err(SqlitePathError::NotDatabaseFile(display))
        }
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
    fn accepts_empty_existing_file() {
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
    fn accepts_sqlite_file_by_header() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("History");
        fs::write(&path, b"SQLite format 3\0rest").unwrap();

        assert!(
            FsSqlitePathValidator
                .validate_database_path(path.to_str().unwrap())
                .is_ok()
        );
    }

    #[test]
    fn rejects_readable_non_sqlite_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, b"not a sqlite db at all").unwrap();

        assert!(matches!(
            FsSqlitePathValidator.validate_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::NotDatabaseFile(_))
        ));
    }

    #[test]
    fn rejects_short_non_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("partial.db");
        fs::write(&path, b"SQLite").unwrap();

        assert!(matches!(
            FsSqlitePathValidator.validate_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::NotDatabaseFile(_))
        ));
    }

    #[test]
    fn canonicalizes_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        fs::write(&path, b"").unwrap();

        let canonical_path = FsSqlitePathValidator
            .canonicalize_database_path(path.to_str().unwrap())
            .unwrap();

        assert_eq!(canonical_path, fs::canonicalize(path).unwrap());
    }

    #[test]
    fn canonicalize_reports_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.db");

        assert!(matches!(
            FsSqlitePathValidator.canonicalize_database_path(path.to_str().unwrap()),
            Err(SqlitePathError::FileNotFound(_))
        ));
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
