use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigWriterError {
    #[error("cache directory is unavailable")]
    MissingCacheDir,
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
}

impl From<std::io::Error> for ConfigWriterError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub trait ConfigWriter: Send + Sync {
    fn get_cache_dir(&self, project_name: &str) -> Result<PathBuf, ConfigWriterError>;
}
