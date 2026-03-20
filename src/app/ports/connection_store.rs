use std::path::PathBuf;

use crate::domain::connection::{ConnectionId, ConnectionProfile};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionStoreError {
    #[error("Config version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Connection name already exists: {0}")]
    DuplicateName(String),
    #[error("Connection not found: {0}")]
    NotFound(String),
}

#[cfg_attr(test, mockall::automock)]
pub trait ConnectionStore: Send + Sync {
    fn load(&self) -> Result<Option<ConnectionProfile>, ConnectionStoreError>;

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError>;

    fn storage_path(&self) -> PathBuf;

    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError>;

    fn find_by_id(
        &self,
        id: &ConnectionId,
    ) -> Result<Option<ConnectionProfile>, ConnectionStoreError>;

    fn delete(&self, id: &ConnectionId) -> Result<(), ConnectionStoreError>;
}
