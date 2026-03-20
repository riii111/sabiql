use std::path::PathBuf;

use crate::domain::connection::ServiceEntry;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ServiceFileError {
    #[error("Service file not found: {0}")]
    NotFound(String),
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[cfg_attr(test, mockall::automock)]
pub trait ServiceFileReader: Send + Sync {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError>;
}
