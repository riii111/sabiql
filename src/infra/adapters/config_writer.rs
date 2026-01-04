use std::path::PathBuf;

use color_eyre::eyre::Result;

use crate::app::ports::ConfigWriter;
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
    fn get_cache_dir(&self, project_name: &str) -> Result<PathBuf> {
        get_cache_dir(project_name)
    }
}
