use std::path::PathBuf;

use crate::app::ports::{ConfigWriter, ConfigWriterError};
use crate::infra::config::cache::{CacheDirError, get_cache_dir};

pub struct FileConfigWriter;

impl FileConfigWriter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileConfigWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn map_cache_dir_error(error: CacheDirError) -> ConfigWriterError {
    match error {
        CacheDirError::BaseDirUnavailable => ConfigWriterError::MissingCacheDir,
        CacheDirError::Io(error) => error.into(),
    }
}

impl ConfigWriter for FileConfigWriter {
    fn get_cache_dir(
        &self,
        project_name: &str,
    ) -> Result<PathBuf, ConfigWriterError> {
        get_cache_dir(project_name).map_err(map_cache_dir_error)
    }
}
