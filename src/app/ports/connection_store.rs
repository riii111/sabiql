use std::path::PathBuf;

use crate::domain::connection::ConnectionProfile;

#[derive(Debug, Clone)]
pub enum ConnectionStoreError {
    /// Config file version doesn't match current version
    VersionMismatch { found: u32, expected: u32 },
    ReadError(String),
    WriteError(String),
    InvalidFormat(String),
    IoError(String),
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
        }
    }
}

impl std::error::Error for ConnectionStoreError {}

/// Port for persisting connection profiles to OS-standard config directory
pub trait ConnectionStore: Send + Sync {
    /// Returns None if no profile exists yet (first run)
    fn load(&self) -> Result<Option<ConnectionProfile>, ConnectionStoreError>;

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError>;

    fn storage_path(&self) -> PathBuf;
}
