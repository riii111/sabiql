use std::path::PathBuf;

use color_eyre::eyre::Result;

pub trait ConfigWriter: Send + Sync {
    fn get_cache_dir(&self, project_name: &str) -> Result<PathBuf>;
}
