use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "the secret store boundary is shared only within the infrastructure crate"
)]
pub(crate) enum SecretStoreError {
    #[error("OS secret store is not supported on this platform")]
    UnsupportedPlatform,
    #[error("OS secret store operation failed")]
    OperationFailed,
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the secret store boundary is shared only within the infrastructure crate"
)]
pub(crate) trait SecretStore: Send + Sync {
    fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError>;

    fn get(&self, reference: &str) -> Result<String, SecretStoreError>;

    fn delete(&self, reference: &str) -> Result<(), SecretStoreError>;
}

#[cfg(target_os = "macos")]
use apple_native_keyring_store::keychain;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use dbus_secret_service_keyring_store as secret_service;
#[cfg(target_os = "windows")]
use windows_native_keyring_store as credential_manager;

const SERVICE_NAME: &str = "com.sabiql.connections";

#[allow(
    clippy::redundant_pub_crate,
    reason = "the adapter is re-exported within the infrastructure crate"
)]
pub(crate) struct PlatformSecretStore {
    operation_lock: Mutex<()>,
}

impl PlatformSecretStore {
    pub(crate) fn new() -> Self {
        Self {
            operation_lock: Mutex::new(()),
        }
    }
}

impl Default for PlatformSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for PlatformSecretStore {
    fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError> {
        let _guard = self
            .operation_lock
            .lock()
            .expect("secret store operation lock poisoned");
        let entry = entry(reference)?;
        entry
            .set_password(secret)
            .map_err(|_| SecretStoreError::OperationFailed)
    }

    fn get(&self, reference: &str) -> Result<String, SecretStoreError> {
        let _guard = self
            .operation_lock
            .lock()
            .expect("secret store operation lock poisoned");
        let entry = entry(reference)?;
        entry
            .get_password()
            .map_err(|_| SecretStoreError::OperationFailed)
    }

    fn delete(&self, reference: &str) -> Result<(), SecretStoreError> {
        let _guard = self
            .operation_lock
            .lock()
            .expect("secret store operation lock poisoned");
        let entry = entry(reference)?;
        entry
            .delete_credential()
            .map_err(|_| SecretStoreError::OperationFailed)
    }
}

fn entry(reference: &str) -> Result<keyring_core::Entry, SecretStoreError> {
    ensure_default_store()?;
    keyring_core::Entry::new(SERVICE_NAME, reference).map_err(|_| SecretStoreError::OperationFailed)
}

fn ensure_default_store() -> Result<(), SecretStoreError> {
    static INITIALIZED: OnceLock<Result<(), SecretStoreError>> = OnceLock::new();

    INITIALIZED
        .get_or_init(|| {
            #[cfg(target_os = "macos")]
            {
                let store =
                    keychain::Store::new().map_err(|_| SecretStoreError::OperationFailed)?;
                keyring_core::set_default_store(store);
                return Ok(());
            }

            #[cfg(target_os = "windows")]
            {
                let store = credential_manager::Store::new()
                    .map_err(|_| SecretStoreError::OperationFailed)?;
                keyring_core::set_default_store(store);
                return Ok(());
            }

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            {
                let store =
                    secret_service::Store::new().map_err(|_| SecretStoreError::OperationFailed)?;
                keyring_core::set_default_store(store);
                return Ok(());
            }

            #[allow(
                unreachable_code,
                reason = "all supported platform branches return above"
            )]
            Err(SecretStoreError::UnsupportedPlatform)
        })
        .clone()
}
