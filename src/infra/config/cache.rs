use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CacheDirError {
    #[error("cache directory is unavailable")]
    BaseDirUnavailable,
    #[error("i/o error: {0}")]
    Io(#[source] io::Error),
}

impl From<io::Error> for CacheDirError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn get_cache_dir(project_name: &str) -> Result<PathBuf, CacheDirError> {
    let cache_base = dirs::cache_dir()
        .ok_or(CacheDirError::BaseDirUnavailable)?;
    let cache_dir = cache_base.join("sabiql").join(project_name);

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}
