use crate::app::ports::outbound::DbOperationError;
use crate::domain::sqlite_path_from_dsn;

use super::SqliteAdapter;

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) fn path_from_dsn(dsn: &str) -> Result<&str, DbOperationError> {
        sqlite_path_from_dsn(dsn)
            .ok_or_else(|| DbOperationError::ConnectionFailed(format!("Invalid SQLite DSN: {dsn}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_path_from_sqlite_dsn() {
        assert_eq!(
            SqliteAdapter::path_from_dsn("sqlite:///tmp/app.db").unwrap(),
            "/tmp/app.db"
        );
    }

    #[test]
    fn rejects_empty_sqlite_dsn() {
        assert!(matches!(
            SqliteAdapter::path_from_dsn("sqlite://"),
            Err(DbOperationError::ConnectionFailed(_))
        ));
    }
}
