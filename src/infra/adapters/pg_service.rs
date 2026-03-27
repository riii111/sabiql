use std::path::PathBuf;

use crate::app::ports::service_file::{ServiceFileError, ServiceFileReader};
use crate::domain::connection::ServiceEntry;

#[derive(Default)]
pub struct PgServiceFileReader;

impl PgServiceFileReader {
    pub fn new() -> Self {
        Self
    }
}

impl ServiceFileReader for PgServiceFileReader {
    fn read_services(&self) -> Result<(Vec<ServiceEntry>, PathBuf), ServiceFileError> {
        let path = find_service_file()?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ServiceFileError::ReadError(format!("{}: {}", path.display(), e)))?;
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

    if let Some(home) = home_dir() {
        let path = home.join(".pg_service.conf");
        if path.is_file() {
            return Ok(path);
        }
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

    Err(ServiceFileError::NotFound(
        "No pg_service.conf found (checked $PGSERVICEFILE, ~/.pg_service.conf, pg_config --sysconfdir)".to_string(),
    ))
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
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
            current = Some(ServiceEntry {
                service_name: name,
                host: None,
                dbname: None,
                port: None,
                user: None,
            });
            continue;
        }

        if let Some(ref mut entry) = current
            && let Some((key, value)) = line.split_once('=')
        {
            let key = key.trim();
            let value = value.trim();
            match key {
                "host" | "hostaddr" => entry.host = Some(value.to_string()),
                "dbname" => entry.dbname = Some(value.to_string()),
                "port" => entry.port = value.parse().ok(),
                "user" => entry.user = Some(value.to_string()),
                _ => {}
            }
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
    fn single_section_parsed() {
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
        assert_eq!(entries[0].host, Some("localhost".to_string()));
        assert_eq!(entries[0].port, Some(5432));
        assert_eq!(entries[0].dbname, Some("mydb".to_string()));
        assert_eq!(entries[0].user, Some("admin".to_string()));
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
        assert_eq!(entries[0].service_name, "dev");
        assert_eq!(entries[0].host, Some("dev.example.com".to_string()));
        assert_eq!(entries[1].service_name, "prod");
        assert_eq!(entries[1].port, Some(5433));
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
        assert_eq!(entries[0].host, Some("localhost".to_string()));
        assert_eq!(entries[0].port, Some(5432));
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
        assert_eq!(entries[0].host, Some("localhost".to_string()));
        assert_eq!(entries[0].port, Some(5432));
    }

    #[test]
    fn invalid_port_stored_as_none() {
        let content = "\
[mydb]
port=not_a_number
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].port, None);
    }

    #[test]
    fn duplicate_sections_last_wins() {
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
        assert_eq!(entries[0].host, Some("second.example.com".to_string()));
        assert_eq!(entries[0].port, Some(5433));
    }

    #[test]
    fn section_with_no_keys() {
        let content = "\
[empty]
";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "empty");
        assert_eq!(entries[0].host, None);
        assert_eq!(entries[0].dbname, None);
        assert_eq!(entries[0].port, None);
        assert_eq!(entries[0].user, None);
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
        assert_eq!(entries[0].host, Some("localhost".to_string()));
    }

    #[test]
    fn whitespace_around_keys_and_values_trimmed() {
        let content = "\
[mydb]
  host  =  db.example.com
  port  =  5432
";
        let entries = parse(content);
        assert_eq!(entries[0].host, Some("db.example.com".to_string()));
        assert_eq!(entries[0].port, Some(5432));
    }

    #[test]
    fn hostaddr_maps_to_host() {
        let content = "\
[mydb]
hostaddr=192.168.1.1
";
        let entries = parse(content);
        assert_eq!(entries[0].host, Some("192.168.1.1".to_string()));
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
        assert_eq!(entries[0].host, Some("localhost".to_string()));
        assert_eq!(entries[0].dbname, None);
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
