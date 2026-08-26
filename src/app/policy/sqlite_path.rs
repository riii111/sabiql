use crate::domain::SqlitePathError;
use crate::model::connection::error::ConnectionErrorKind;
use crate::ports::outbound::DbOperationError;

pub fn connection_error_kind(error: &SqlitePathError) -> ConnectionErrorKind {
    match error {
        SqlitePathError::FileNotFound(_) => ConnectionErrorKind::SqliteFileNotFound,
        SqlitePathError::IsDirectory(_) => ConnectionErrorKind::SqlitePathIsDirectory,
        SqlitePathError::NotRegularFile(_) => ConnectionErrorKind::SqlitePathNotRegularFile,
        SqlitePathError::NotDatabaseFile(_) => ConnectionErrorKind::SqliteNotDatabaseFile,
        SqlitePathError::ReadAccessDenied(_) => ConnectionErrorKind::SqliteReadAccessDenied,
        SqlitePathError::PathAccessDenied(_) => ConnectionErrorKind::SqlitePathAccessDenied,
        SqlitePathError::Io(_) => ConnectionErrorKind::SqlitePathIo,
    }
}

pub fn to_db_operation_error(error: &SqlitePathError) -> DbOperationError {
    DbOperationError::SqlitePath(error.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_sqlite_path_error_in_db_operation_error() {
        let path_error = SqlitePathError::FileNotFound("/tmp/missing.db".to_string());

        assert!(matches!(
            to_db_operation_error(&path_error),
            DbOperationError::SqlitePath(error) if error == path_error
        ));
    }
}
