use std::io::ErrorKind;
use std::path::Path;

use crate::domain::{SqliteStartupError, SqliteStartupTarget};

pub fn validate_sqlite_startup_file(
    target: &SqliteStartupTarget,
) -> Result<(), SqliteStartupError> {
    validate_sqlite_file_path(Path::new(target.path()))
}

fn validate_sqlite_file_path(path: &Path) -> Result<(), SqliteStartupError> {
    let display = path.display().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(metadata_error(&display, error.kind(), &error.to_string()));
        }
    };

    if metadata.is_dir() {
        return Err(SqliteStartupError::IsDirectory(display));
    }

    Ok(())
}

fn metadata_error(display: &str, kind: ErrorKind, source: &str) -> SqliteStartupError {
    match kind {
        ErrorKind::NotFound => SqliteStartupError::FileNotFound(display.to_string()),
        ErrorKind::PermissionDenied => {
            SqliteStartupError::PathAccessDenied(format!("{display}: {source}"))
        }
        _ => SqliteStartupError::Io(format!("{display}: {source}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    mod metadata_error {
        use super::*;

        #[test]
        fn not_found() {
            let error = metadata_error("/tmp/app.db", ErrorKind::NotFound, "No such file");

            assert_eq!(
                error,
                SqliteStartupError::FileNotFound("/tmp/app.db".to_string())
            );
        }

        #[test]
        fn permission_denied() {
            let error = metadata_error(
                "/tmp/app.db",
                ErrorKind::PermissionDenied,
                "permission denied",
            );

            assert_eq!(
                error,
                SqliteStartupError::PathAccessDenied("/tmp/app.db: permission denied".to_string())
            );
        }

        #[test]
        fn other_io_error() {
            let error = metadata_error("/tmp/app.db", ErrorKind::Other, "device offline");

            assert_eq!(
                error,
                SqliteStartupError::Io("/tmp/app.db: device offline".to_string())
            );
        }
    }

    mod validate_sqlite_startup_file {
        use super::*;

        #[test]
        fn accepts_existing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();
            let target = SqliteStartupTarget::from_cli_input(path.to_str().unwrap()).unwrap();

            assert!(validate_sqlite_startup_file(&target).is_ok());
        }

        #[test]
        fn rejects_missing_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let target = SqliteStartupTarget::from_cli_input(path.to_str().unwrap()).unwrap();

            assert!(matches!(
                validate_sqlite_startup_file(&target),
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
                validate_sqlite_startup_file(&target),
                Err(SqliteStartupError::IsDirectory(_))
            ));
        }
    }
}
