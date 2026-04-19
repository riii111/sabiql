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

impl From<CacheDirError> for ConfigWriterError {
    fn from(error: CacheDirError) -> Self {
        match error {
            CacheDirError::BaseDirUnavailable => Self::MissingCacheDir,
            CacheDirError::Io(error) => error.into(),
        }
    }
}

impl ConfigWriter for FileConfigWriter {
    fn get_cache_dir(
        &self,
        project_name: &str,
    ) -> Result<PathBuf, ConfigWriterError> {
        Ok(get_cache_dir(project_name)?)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn unavailable_cache_base_maps_to_missing_cache_dir() {
        let error: ConfigWriterError = CacheDirError::BaseDirUnavailable.into();

        assert!(matches!(error, ConfigWriterError::MissingCacheDir));
    }

    #[test]
    fn io_not_found_remains_io_error() {
        let error: ConfigWriterError = CacheDirError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "missing parent",
        ))
        .into();

        match error {
            ConfigWriterError::Io(source) => {
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            ConfigWriterError::MissingCacheDir => panic!("expected io error"),
        }
    }
}
