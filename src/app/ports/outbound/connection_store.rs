use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::connection::{ConnectionId, ConnectionProfile, ConnectionProfileError};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionStoreError {
    #[error("Config version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
    #[error("IO error: {0}")]
    Io(#[source] Arc<std::io::Error>),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[source] Arc<toml::ser::Error>),
    #[error("TOML deserialize error: {0}")]
    TomlDeserialize(#[source] Arc<toml::de::Error>),
    #[error("Invalid profile: {0}")]
    InvalidProfile(#[source] ConnectionProfileError),
    #[error("Connection name already exists: {0}")]
    DuplicateName(String),
    #[error("Connection not found: {0}")]
    NotFound(String),
}

impl From<std::io::Error> for ConnectionStoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(Arc::new(e))
    }
}

impl From<toml::ser::Error> for ConnectionStoreError {
    fn from(e: toml::ser::Error) -> Self {
        Self::TomlSerialize(Arc::new(e))
    }
}

impl From<toml::de::Error> for ConnectionStoreError {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlDeserialize(Arc::new(e))
    }
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
