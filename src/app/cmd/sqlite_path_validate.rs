use std::sync::Arc;

use crate::domain::SqlitePathError;
use crate::ports::outbound::SqlitePathValidator;

pub async fn validate_sqlite_database_path(
    validator: &Arc<dyn SqlitePathValidator>,
    path: String,
) -> Result<(), SqlitePathError> {
    let validator = Arc::clone(validator);
    tokio::task::spawn_blocking(move || validator.validate_database_path(&path))
        .await
        .map_err(|error| SqlitePathError::Io(format!("validation task failed: {error}")))?
}
