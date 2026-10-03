use std::sync::Arc;

use crate::domain::connection::{ConnectionId, ConnectionProfile, ConnectionProfileError};

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum SecretStoreFailure {
    #[error("OS password storage is unsupported on this platform")]
    Unsupported,
    #[error("OS password storage is unavailable; check its service and login session")]
    Unavailable,
    #[error(
        "OS password storage access was denied or the store is locked; check its permissions and unlock it"
    )]
    AccessDenied,
    #[error(
        "OS password storage did not respond within 60 seconds; check its prompt, service and login session"
    )]
    TimedOut,
}

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
    #[error("Invalid password reference in saved settings")]
    InvalidPasswordReference(String),
    #[error("Duplicate password reference in saved settings")]
    DuplicatePasswordReference(String),
    #[error(
        "{0}. For PostgreSQL, save an empty password and use .pgpass or a PostgreSQL service. PostgreSQL and MySQL can also use --connection-env. Editing a legacy plaintext password requires secure storage, or clearing the password first."
    )]
    SecretStore(SecretStoreFailure),
    #[error(
        "Configuration changes were saved, but old password cleanup could not be confirmed: {0}. An old credential may remain in the OS password store; verify it before removing it manually."
    )]
    CleanupIncomplete(SecretStoreFailure),
    #[error("Connection changed while saving; reload it and try again")]
    ConcurrentModification,
}

impl ConnectionStoreError {
    pub fn load_failure_message(&self) -> String {
        let reason = match self {
        Self::VersionMismatch { found, expected } => format!(
            "Configuration version v{found} is not supported (maximum v{expected}). Update sabiql to a compatible version."
        ),
        Self::SecretStore(reason) => format!(
            "Saved connections could not be loaded: {reason}. Check the OS password store, its lock, service and login session; restart sabiql after recovery."
        ),
        Self::InvalidPasswordReference(_)
        | Self::DuplicatePasswordReference(_) =>
            "Saved connections contain invalid or duplicate password references. Restore valid references from a trusted backup or correct the configuration before restarting sabiql.".to_string(),
        _ => "Saved connections could not be loaded. Check the configuration and its file permissions, then restart sabiql.".to_string(),
    };
        format!(
            "{reason} Your configuration file is preserved; do not delete it or re-register connections to recover."
        )
    }
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
    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError>;

    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError>;

    fn find_by_id(
        &self,
        id: &ConnectionId,
    ) -> Result<Option<ConnectionProfile>, ConnectionStoreError>;

    fn delete(&self, id: &ConnectionId) -> Result<(), ConnectionStoreError>;
}
