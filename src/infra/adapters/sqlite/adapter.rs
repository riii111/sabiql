use super::sqlite3::SqliteCli;

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
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}
