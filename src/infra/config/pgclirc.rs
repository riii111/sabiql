use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

/// Generate a pgclirc config file for per-project history.
pub fn generate_pgclirc(cache_dir: &Path) -> Result<PathBuf> {
    let pgclirc_path = cache_dir.join("pgclirc");
    let history_path = cache_dir.join("pgcli_history");

    let content = format!(
        "[main]\nhistory_file = {}\n",
        history_path.display()
    );

    fs::create_dir_all(cache_dir)?;
    fs::write(&pgclirc_path, content)?;
    Ok(pgclirc_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn generate_pgclirc_creates_file_with_correct_content() {
        let temp_dir = tempdir().unwrap();
        let cache_dir = temp_dir.path();

        let result = generate_pgclirc(cache_dir);

        assert!(result.is_ok());
        let pgclirc_path = result.unwrap();
        assert!(pgclirc_path.exists());

        let content = fs::read_to_string(&pgclirc_path).unwrap();
        assert!(content.contains("[main]"));
        assert!(content.contains("history_file"));
        assert!(content.contains("pgcli_history"));
    }

    #[test]
    fn generate_pgclirc_creates_parent_directories() {
        let temp_dir = tempdir().unwrap();
        let cache_dir = temp_dir.path().join("nested").join("path");

        let result = generate_pgclirc(&cache_dir);

        assert!(result.is_ok());
        assert!(cache_dir.exists());
    }
}
