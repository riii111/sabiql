use crate::domain::SqlitePathError;
use crate::ports::outbound::DbOperationError;

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
