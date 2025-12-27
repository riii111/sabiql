use std::fs;
use std::path::PathBuf;

use color_eyre::eyre::{Result, eyre};

pub fn get_cache_dir(project_name: &str) -> Result<PathBuf> {
    let cache_base = dirs::cache_dir().ok_or_else(|| eyre!("Could not find cache directory"))?;
    let cache_dir = cache_base.join("dbtui").join(project_name);

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}
