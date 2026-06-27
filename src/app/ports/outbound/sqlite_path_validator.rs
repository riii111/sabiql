use crate::domain::SqlitePathError;

pub trait SqlitePathValidator: Send + Sync {
    fn validate_database_path(&self, path: &str) -> Result<(), SqlitePathError>;
}
