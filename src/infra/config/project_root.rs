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
        if current.join(dirname).exists() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    mod find_dir_upward_tests {
        use super::*;

        #[test]
        fn finds_root_with_git_directory() {
            let temp_dir = tempfile::tempdir().unwrap();
            fs::create_dir(temp_dir.path().join(".git")).unwrap();

            let result = find_dir_upward(temp_dir.path(), ".git");

            assert_eq!(result, Some(temp_dir.path().to_path_buf()));
        }

        #[test]
        fn finds_root_with_git_file_worktree() {
            let temp_dir = tempfile::tempdir().unwrap();
            fs::write(
                temp_dir.path().join(".git"),
                "gitdir: /some/path/.git/worktrees/branch",
            )
            .unwrap();

            let result = find_dir_upward(temp_dir.path(), ".git");

            assert_eq!(result, Some(temp_dir.path().to_path_buf()));
        }

        #[test]
        fn finds_root_from_nested_subdirectory() {
            let temp_dir = tempfile::tempdir().unwrap();
            fs::create_dir(temp_dir.path().join(".git")).unwrap();
            let nested = temp_dir.path().join("src").join("deep");
            fs::create_dir_all(&nested).unwrap();

            let result = find_dir_upward(&nested, ".git");

            assert_eq!(result, Some(temp_dir.path().to_path_buf()));
        }
    }

    mod get_project_name_tests {
        use super::*;

        #[test]
        fn extracts_dir_basename() {
            assert_eq!(get_project_name(Path::new("/foo/bar")), "bar");
        }

        #[test]
        fn returns_unknown_for_root() {
            assert_eq!(get_project_name(Path::new("/")), "unknown");
        }
    }
}
