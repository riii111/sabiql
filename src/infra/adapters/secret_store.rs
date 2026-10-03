use std::sync::OnceLock;

use crate::app::ports::outbound::connection_store::{ConnectionStoreError, SecretStoreFailure};
use serde::{Deserialize, Serialize};

#[path = "secret_store_process.rs"]
mod process;
pub use process::run_secret_store_helper;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum SecretStoreError {
    #[error("OS secret store entry does not exist")]
    NoEntry,
    #[error("OS secret store is not supported on this platform")]
    UnsupportedPlatform,
    #[error("OS secret store operation failed")]
    OperationFailed,
    #[error("OS secret store access denied or locked")]
    AccessDenied,
    #[error("OS secret store timed out")]
    TimedOut,
}

pub(super) trait SecretStore: Send + Sync {
    fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError>;

    fn get(&self, reference: &str) -> Result<String, SecretStoreError>;

    fn delete(&self, reference: &str) -> Result<(), SecretStoreError>;
}

#[cfg(target_os = "macos")]
use apple_native_keyring_store::keychain;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use dbus_secret_service_keyring_store as secret_service;
#[cfg(target_os = "macos")]
use security_framework::base::Error as SecurityError;
#[cfg(target_os = "windows")]
use windows_native_keyring_store as credential_manager;

const SERVICE_NAME: &str = "com.sabiql.connections";

pub(super) struct PlatformSecretStore;

impl PlatformSecretStore {
    pub(super) fn new() -> Self {
        Self
    }
}

impl SecretStore for PlatformSecretStore {
    fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError> {
        process::call(process::Request::Set {
            reference: reference.into(),
            secret: secret.into(),
        })
        .map(|_| ())
    }

    fn get(&self, reference: &str) -> Result<String, SecretStoreError> {
        process::call(process::Request::Get {
            reference: reference.into(),
        })
    }

    fn delete(&self, reference: &str) -> Result<(), SecretStoreError> {
        process::call(process::Request::Delete {
            reference: reference.into(),
        })
        .map(|_| ())
    }
}

impl From<SecretStoreError> for ConnectionStoreError {
    fn from(error: SecretStoreError) -> Self {
        Self::SecretStore(error.into())
    }
}

impl From<SecretStoreError> for SecretStoreFailure {
    fn from(error: SecretStoreError) -> Self {
        match error {
            SecretStoreError::UnsupportedPlatform => Self::Unsupported,
            SecretStoreError::AccessDenied => Self::AccessDenied,
            SecretStoreError::TimedOut => Self::TimedOut,
            SecretStoreError::OperationFailed | SecretStoreError::NoEntry => Self::Unavailable,
        }
    }
}

fn map_keyring_error(error: keyring_core::Error) -> SecretStoreError {
    #[cfg(target_os = "macos")]
    if let keyring_core::Error::PlatformFailure(cause) | keyring_core::Error::NoStorageAccess(cause) =
        &error
        && let Some(cause) = cause.downcast_ref::<SecurityError>()
    {
        return match cause.code() {
            -128 | -61 | -25244 | -25292 | -25293 | -25308 | -25315 => {
                SecretStoreError::AccessDenied
            }
            _ => SecretStoreError::OperationFailed,
        };
    }
    // The Windows adapter's numeric error wrapper is private. Match only its
    // fixed access-denied representation; never surface the underlying text.
    #[cfg(target_os = "windows")]
    if let keyring_core::Error::PlatformFailure(cause) = &error
        && cause.to_string() == "Windows error code 5"
    {
        return SecretStoreError::AccessDenied;
    }
    match error {
        keyring_core::Error::NoEntry => SecretStoreError::NoEntry,
        keyring_core::Error::NoStorageAccess(_) => SecretStoreError::AccessDenied,
        keyring_core::Error::NotSupportedByStore(_) => SecretStoreError::UnsupportedPlatform,
        _ => SecretStoreError::OperationFailed,
    }
}

fn entry(reference: &str) -> Result<keyring_core::Entry, SecretStoreError> {
    ensure_default_store()?;
    keyring_core::Entry::new(SERVICE_NAME, reference).map_err(map_keyring_error)
}

fn ensure_default_store() -> Result<(), SecretStoreError> {
    static INITIALIZED: OnceLock<Result<(), SecretStoreError>> = OnceLock::new();

    INITIALIZED
        .get_or_init(|| {
            #[cfg(target_os = "macos")]
            {
                let store = keychain::Store::new().map_err(map_keyring_error)?;
                keyring_core::set_default_store(store);
                return Ok(());
            }

            #[cfg(target_os = "windows")]
            {
                let store = credential_manager::Store::new().map_err(map_keyring_error)?;
                keyring_core::set_default_store(store);
                return Ok(());
            }

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            {
                let store = secret_service::Store::new().map_err(map_keyring_error)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_categories_discard_underlying_secret_bearing_details() {
        let errors = [
            (
                keyring_core::Error::NotSupportedByStore("fake secret".into()),
                SecretStoreError::UnsupportedPlatform,
            ),
            (
                keyring_core::Error::NoStorageAccess(Box::new(std::io::Error::other(
                    "fake secret",
                ))),
                SecretStoreError::AccessDenied,
            ),
            (
                keyring_core::Error::PlatformFailure(Box::new(std::io::Error::other(
                    "fake secret",
                ))),
                SecretStoreError::OperationFailed,
            ),
        ];
        for (input, expected) in errors {
            let error = map_keyring_error(input);
            assert_eq!(error, expected);
            assert!(
                !ConnectionStoreError::from(error)
                    .to_string()
                    .contains("fake secret")
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_access_denied_is_distinct_from_service_failure() {
        let error = keyring_core::Error::PlatformFailure(Box::new(std::io::Error::other(
            "Windows error code 5",
        )));
        assert_eq!(map_keyring_error(error), SecretStoreError::AccessDenied);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_denial_and_cancel_are_distinct_from_missing_keychain() {
        for code in [-128, -25293, -25308, -25315] {
            let error = keychain::decode_error(SecurityError::from_code(code));
            assert_eq!(map_keyring_error(error), SecretStoreError::AccessDenied);
        }
        let error = keychain::decode_error(SecurityError::from_code(-25291));
        assert_eq!(map_keyring_error(error), SecretStoreError::OperationFailed);
    }
}
