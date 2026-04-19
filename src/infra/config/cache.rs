use std::fs;
use std::io;
use std::path::PathBuf;

pub fn get_cache_dir(project_name: &str) -> io::Result<PathBuf> {
    let cache_base = dirs::cache_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find cache directory"))?;
    let cache_dir = cache_base.join("sabiql").join(project_name);

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}
