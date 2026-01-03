use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

pub trait ConfigWriter: Send + Sync {
    fn get_cache_dir(&self, project_name: &str) -> Result<PathBuf>;
    fn generate_pgclirc(&self, cache_dir: &Path) -> Result<PathBuf>;
}
