use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::outbound::service_file::{PgServiceEntryReader, ServiceFileError};
use crate::domain::connection::ServiceEntry;

#[derive(Default)]
pub struct PgServiceFileReader;

impl PgServiceFileReader {
    pub fn new() -> Self {
        Self
    }
}

impl PgServiceEntryReader for PgServiceFileReader {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError> {
        let path = find_service_file()?;
        let content =
            std::fs::read_to_string(&path).map_err(|source| ServiceFileError::ReadAt {
                path: path.clone(),
                source: Arc::new(source),
            })?;
        let entries = parse(&content);
        Ok((entries, path))
    }
}

fn find_service_file() -> Result<PathBuf, ServiceFileError> {
    if let Ok(val) = std::env::var("PGSERVICEFILE") {
        let path = PathBuf::from(&val);
        if path.is_file() {
            return Ok(path);
        }
        return Err(ServiceFileError::NotFound(format!(
            "PGSERVICEFILE={val} does not exist"
        )));
    }

    let user_service_path = user_service_file_path();
    if let Some(path) = &user_service_path
        && path.is_file()
    {
        return Ok(path.clone());
    }

    if let Some(output) = std::process::Command::new("pg_config")
        .arg("--sysconfdir")
        .output()
        .ok()
        .filter(|o| o.status.success())
    {
        let sysconfdir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = PathBuf::from(&sysconfdir).join("pg_service.conf");
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(ServiceFileError::NotFound(service_file_not_found_message(
        user_service_path.as_deref(),
    )))
}

fn service_file_not_found_message(user_service_path: Option<&Path>) -> String {
    let user_path_hint = user_service_path.map_or_else(
        || "the platform user config path".to_string(),
        |path| path.display().to_string(),
    );
    format!(
        "No pg_service.conf found (checked PGSERVICEFILE, {user_path_hint}, and pg_config --sysconfdir)"
    )
}

#[cfg(target_os = "windows")]
fn user_service_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|path| windows_user_service_file_path(&path))
}

#[cfg(not(target_os = "windows"))]
fn user_service_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|path| unix_user_service_file_path(&path))
}

#[cfg(any(target_os = "windows", test))]
fn windows_user_service_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join("postgresql").join(".pg_service.conf")
}

#[cfg(any(not(target_os = "windows"), test))]
fn unix_user_service_file_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".pg_service.conf")
}

fn parse(content: &str) -> Vec<ServiceEntry> {
    let mut entries: Vec<ServiceEntry> = Vec::new();
    let mut current: Option<ServiceEntry> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            let name = line[1..line.len() - 1].trim().to_string();
            current = Some(ServiceEntry { service_name: name });
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    // Duplicate sections: last one wins (PostgreSQL convention)
    let mut seen = std::collections::HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        seen.insert(entry.service_name.clone(), i);
    }
    let mut unique_indices: Vec<usize> = seen.into_values().collect();
    unique_indices.sort_unstable();
    unique_indices
        .into_iter()
        .map(|i| entries[i].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Guards env-var–mutating tests so they don't race each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn empty_content_returns_no_entries() {
        assert_eq!(parse(""), Vec::new());
    }

    #[test]
    fn single_section_returns_service_name() {
        let content = "\
[mydb]
host=localhost
port=5432
dbname=mydb
user=admin
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn multiple_sections_parsed() {
        let content = "\
[dev]
host=dev.example.com
dbname=devdb

[prod]
host=prod.example.com
dbname=proddb
port=5433
";
        let entries = parse(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.service_name.as_str())
                .collect::<Vec<_>>(),
            ["dev", "prod"]
        );
    }

    #[test]
    fn comments_and_blank_lines_ignored() {
        let content = "\
# This is a comment
; Another comment

[mydb]
host=localhost

# inline section comment
port=5432
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn invalid_lines_ignored() {
        let content = "\
[mydb]
host=localhost
this is not a valid line
port=5432
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn duplicate_sections_are_collapsed() {
        let content = "\
[mydb]
host=first.example.com
port=5432

[mydb]
host=second.example.com
port=5433
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn section_with_no_keys() {
        let content = "\
[empty]
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "empty");
    }

    #[test]
    fn keys_before_any_section_ignored() {
        let content = "\
host=orphan
port=1234

[mydb]
host=localhost
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn unknown_keys_ignored() {
        let content = "\
[mydb]
host=localhost
sslmode=require
connect_timeout=10
application_name=myapp
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn windows_user_service_file_uses_postgresql_appdata_directory() {
        let config_dir = PathBuf::from(r"C:\Users\test\AppData\Roaming");

        let path = windows_user_service_file_path(&config_dir);

        assert_eq!(path, config_dir.join("postgresql").join(".pg_service.conf"));
    }

    #[test]
    fn unix_user_service_file_uses_home_directory() {
        let home_dir = PathBuf::from("/home/test");

        let path = unix_user_service_file_path(&home_dir);

        assert_eq!(path, home_dir.join(".pg_service.conf"));
    }

    #[test]
    fn not_found_message_includes_resolved_user_service_file_path() {
        let path = Path::new(r"C:\Users\test\AppData\Roaming\postgresql\.pg_service.conf");

        let message = service_file_not_found_message(Some(path));

        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn find_service_file_uses_pgservicefile_env() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let tmpdir = std::env::temp_dir();
        let path = tmpdir.join("test_pg_service.conf");
        std::fs::write(&path, "[test]\nhost=localhost\n").unwrap();

        let original = std::env::var("PGSERVICEFILE").ok();
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var("PGSERVICEFILE", &path) };

        let result = find_service_file();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), path);

        unsafe {
            match original {
                Some(val) => std::env::set_var("PGSERVICEFILE", val),
                None => std::env::remove_var("PGSERVICEFILE"),
            }
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn find_service_file_errors_when_pgservicefile_missing() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let original = std::env::var("PGSERVICEFILE").ok();
        // SAFETY: test-only, serialized by ENV_LOCK
        unsafe { std::env::set_var("PGSERVICEFILE", "/nonexistent/path/pg_service.conf") };

        let result = find_service_file();
        assert!(result.is_err());

        unsafe {
            match original {
                Some(val) => std::env::set_var("PGSERVICEFILE", val),
                None => std::env::remove_var("PGSERVICEFILE"),
            }
        }
    }
}
