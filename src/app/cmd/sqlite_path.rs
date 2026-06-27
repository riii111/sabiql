use std::path::Path;

use crate::domain::{SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error};

pub fn validate_sqlite_database_path(path: &Path) -> Result<(), SqlitePathError> {
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

    match std::fs::File::open(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(classify_sqlite_read_error(
            &display,
            error.kind(),
            &error.to_string(),
        )),
    }
}

pub fn validate_sqlite_database_path_str(path: &str) -> Result<(), SqlitePathError> {
    validate_sqlite_database_path(Path::new(path))
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

        assert!(validate_sqlite_database_path(&path).is_ok());
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.db");

        assert!(matches!(
            validate_sqlite_database_path(&path),
            Err(SqlitePathError::FileNotFound(_))
        ));
    }

    #[test]
    fn rejects_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folder.db");
        fs::create_dir(&path).unwrap();

        assert!(matches!(
            validate_sqlite_database_path(&path),
            Err(SqlitePathError::IsDirectory(_))
        ));
    }
}
