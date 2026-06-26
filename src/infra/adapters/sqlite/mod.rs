use crate::app::ports::outbound::DbOperationError;

mod sql;
mod sqlite3;

use sqlite3::SqliteCli;
const MAIN_SCHEMA: &str = "main";

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    pub(in crate::adapters::sqlite) cli: SqliteCli,
}

impl SqliteAdapter {
    pub fn new() -> Self {
        Self {
            cli: SqliteCli::new(),
        }
    }

    pub(in crate::adapters::sqlite) fn path_from_dsn(dsn: &str) -> Result<&str, DbOperationError> {
        dsn.strip_prefix("sqlite://")
            .filter(|path| !path.is_empty())
            .ok_or_else(|| DbOperationError::ConnectionFailed(format!("Invalid SQLite DSN: {dsn}")))
    }
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}
