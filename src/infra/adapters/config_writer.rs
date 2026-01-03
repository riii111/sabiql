use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

use crate::app::ports::ConfigWriter;
use crate::infra::config::{cache::get_cache_dir, pgclirc::generate_pgclirc};

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

    fn generate_pgclirc(&self, cache_dir: &Path) -> Result<PathBuf> {
        generate_pgclirc(cache_dir)
    }
}
