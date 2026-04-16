use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum FolderOpenError {
    #[error("{0}")]
    Spawn(Arc<std::io::Error>),
}

impl From<std::io::Error> for FolderOpenError {
    fn from(e: std::io::Error) -> Self {
        Self::Spawn(Arc::new(e))
    }
}

pub trait FolderOpener: Send + Sync {
    fn open(&self, path: &Path) -> Result<(), FolderOpenError>;
}
