use std::env;
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

pub fn find_project_root() -> Result<PathBuf> {
    let cwd = env::current_dir()?;

    if let Some(root) = find_dir_upward(&cwd, ".git") {
        return Ok(root);
    }

    Ok(cwd)
}

fn find_dir_upward(start: &Path, dirname: &str) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(dirname).is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn get_project_name(root: &Path) -> String {
    root.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}
