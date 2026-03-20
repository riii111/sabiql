use std::path::Path;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct FolderOpenError {
    pub message: String,
}

pub trait FolderOpener: Send + Sync {
    fn open(&self, path: &Path) -> Result<(), FolderOpenError>;
}
