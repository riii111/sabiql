use std::path::PathBuf;

use crate::domain::SqlitePathError;

pub trait SqlitePathValidator: Send + Sync {
    fn validate_database_path(&self, path: &str) -> Result<(), SqlitePathError>;
    fn canonicalize_database_path(&self, path: &str) -> Result<PathBuf, SqlitePathError>;
}
