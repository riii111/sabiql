use std::io;
use std::path::PathBuf;

use crate::app::ports::{ConfigWriter, ConfigWriterError};
use crate::infra::config::cache::get_cache_dir;

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

impl ConfigWriter for FileConfigWriter {
    fn get_cache_dir(
        &self,
        project_name: &str,
    ) -> Result<PathBuf, ConfigWriterError> {
        get_cache_dir(project_name).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ConfigWriterError::MissingCacheDir
            } else {
                error.into()
            }
        })
    }
}
