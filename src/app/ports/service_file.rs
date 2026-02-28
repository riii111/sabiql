use std::path::PathBuf;

use crate::domain::connection::ServiceEntry;

#[derive(Debug, Clone)]
pub enum ServiceFileError {
    NotFound(String),
    ReadError(String),
    ParseError(String),
}

impl std::fmt::Display for ServiceFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Service file not found: {}", msg),
            Self::ReadError(msg) => write!(f, "Read error: {}", msg),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for ServiceFileError {}

#[cfg_attr(test, mockall::automock)]
pub trait ServiceFileReader: Send + Sync {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError>;
}
