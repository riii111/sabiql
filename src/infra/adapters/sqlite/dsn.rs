use crate::app::ports::outbound::DbOperationError;

use super::SqliteAdapter;

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) fn path_from_dsn(dsn: &str) -> Result<&str, DbOperationError> {
        dsn.strip_prefix("sqlite://")
            .filter(|path| !path.is_empty())
            .ok_or_else(|| DbOperationError::ConnectionFailed(format!("Invalid SQLite DSN: {dsn}")))
    }
}
