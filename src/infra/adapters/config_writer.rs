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

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn unavailable_cache_base_maps_to_missing_cache_dir() {
        let error = map_cache_dir_error(CacheDirError::BaseDirUnavailable);

        assert!(matches!(error, ConfigWriterError::MissingCacheDir));
    }

    #[test]
    fn io_not_found_remains_io_error() {
        let error = map_cache_dir_error(CacheDirError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "missing parent",
        )));

        match error {
            ConfigWriterError::Io(source) => {
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            ConfigWriterError::MissingCacheDir => panic!("expected io error"),
        }
    }
}
