use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigWriterError {
    #[error("cache directory is unavailable")]
    MissingCacheDir,
    #[error("I/O error: {0}")]
    Io(#[source] Arc<std::io::Error>),
}

impl From<std::io::Error> for ConfigWriterError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

pub trait ConfigWriter: Send + Sync {
    fn get_cache_dir(&self, project_name: &str) -> Result<PathBuf, ConfigWriterError>;
}
