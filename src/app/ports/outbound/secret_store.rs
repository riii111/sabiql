#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SecretStoreError {
    #[error("OS secret store is not supported on this platform")]
    UnsupportedPlatform,
    #[error("OS secret store operation failed")]
    OperationFailed,
}

#[cfg_attr(test, mockall::automock)]
pub trait SecretStore: Send + Sync {
    fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError>;

    fn get(&self, reference: &str) -> Result<String, SecretStoreError>;

    fn delete(&self, reference: &str) -> Result<(), SecretStoreError>;
}
