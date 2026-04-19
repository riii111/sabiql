use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::connection::ServiceEntry;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ServiceFileError {
    #[error("Service file not found: {0}")]
    NotFound(String),
    #[error("Failed to read {path}: {source}", path = path.display())]
    ReadAt {
        path: PathBuf,
        #[source]
        source: Arc<std::io::Error>,
    },
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[cfg_attr(test, mockall::automock)]
pub trait PgServiceEntryReader: Send + Sync {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError>;
}
