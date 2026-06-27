use crate::domain::SqlitePathError;
use crate::model::connection::error::ConnectionErrorKind;
use crate::ports::outbound::DbOperationError;

pub fn connection_error_kind(error: &SqlitePathError) -> ConnectionErrorKind {
    match error {
        SqlitePathError::FileNotFound(_) => ConnectionErrorKind::SqliteFileNotFound,
        SqlitePathError::IsDirectory(_) => ConnectionErrorKind::SqlitePathIsDirectory,
        SqlitePathError::NotRegularFile(_) => ConnectionErrorKind::SqlitePathNotRegularFile,
        SqlitePathError::ReadAccessDenied(_) => ConnectionErrorKind::SqliteReadAccessDenied,
        SqlitePathError::PathAccessDenied(_) => ConnectionErrorKind::SqlitePathAccessDenied,
        SqlitePathError::Io(_) => ConnectionErrorKind::SqlitePathIo,
    }
}

pub fn to_db_operation_error(error: &SqlitePathError) -> DbOperationError {
    DbOperationError::ConnectionFailed(error.to_string())
}
