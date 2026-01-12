use std::path::PathBuf;

use crate::domain::connection::{ConnectionId, ConnectionProfile};

#[derive(Debug, Clone)]
pub enum ConnectionStoreError {
    VersionMismatch { found: u32, expected: u32 },
    ReadError(String),
    WriteError(String),
    InvalidFormat(String),
    IoError(String),
    DuplicateName(String),
    NotFound(String),
}

impl std::fmt::Display for ConnectionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionMismatch { found, expected } => {
                write!(
                    f,
                    "Config version mismatch: found {}, expected {}",
                    found, expected
                )
            }
            Self::ReadError(msg) => write!(f, "Read error: {}", msg),
            Self::WriteError(msg) => write!(f, "Write error: {}", msg),
            Self::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            Self::IoError(msg) => write!(f, "IO error: {}", msg),
            Self::DuplicateName(name) => write!(f, "Connection name already exists: {}", name),
            Self::NotFound(id) => write!(f, "Connection not found: {}", id),
        }
    }
}

impl std::error::Error for ConnectionStoreError {}

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
