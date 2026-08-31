use super::sqlite3::SqliteCli;

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    pub(in crate::adapters::sqlite) cli: SqliteCli,
}

impl SqliteAdapter {
    #[allow(
        clippy::new_without_default,
        reason = "new() is the only default construction API"
    )]
    pub fn new() -> Self {
        Self {
            cli: SqliteCli::new(),
        }
    }
}
