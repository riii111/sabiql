use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::outbound::service_file::{
    PgServiceEntryReader, ServiceFileContents, ServiceFileError,
};
use crate::domain::connection::ServiceEntry;

#[derive(Default)]
pub struct PgServiceFileReader;

impl PgServiceFileReader {
    pub fn new() -> Self {
        Self
    }
}

impl PgServiceEntryReader for PgServiceFileReader {
    fn read_services(&self) -> Result<ServiceFileContents, ServiceFileError> {
        let service_file = std::env::var_os("PGSERVICEFILE").map(PathBuf::from);
        let sysconfdir = std::env::var_os("PGSYSCONFDIR").map(PathBuf::from);
        let (user, system) = service_file_paths(
            service_file.clone(),
            user_service_file_path(),
            sysconfdir,
            || {
                std::process::Command::new("pg_config")
                    .arg("--sysconfdir")
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
            },
        );
        read_service_files(user.as_deref(), service_file.is_some(), system.as_deref())
    }
}

fn service_file_paths(
    service_file: Option<PathBuf>,
    default_user: Option<PathBuf>,
    sysconfdir: Option<PathBuf>,
    default_sysconfdir: impl FnOnce() -> Option<PathBuf>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    (
        service_file.or(default_user),
        sysconfdir
            .or_else(default_sysconfdir)
            .map(|dir| dir.join("pg_service.conf")),
    )
}

fn read_service_files(
    user: Option<&Path>,
    explicit_user: bool,
    system: Option<&Path>,
) -> Result<ServiceFileContents, ServiceFileError> {
    let mut entries = Vec::new();
    let mut found = false;
    if let Some(path) = user
        && let Some(content) = read_service_file(path, explicit_user)?
    {
        found = true;
        entries = parse(&content, path);
    }
    let mut warning = None;
    if let Some(path) = system {
        match read_service_file(path, false) {
            Ok(Some(content)) => {
                found = true;
                for entry in parse(&content, path) {
                    if !entries
                        .iter()
                        .any(|user| user.service_name == entry.service_name)
                    {
                        entries.push(entry);
                    }
                }
            }
            Ok(None) => {}
            Err(error) if !entries.is_empty() => warning = Some(error),
            Err(error) => return Err(error),
        }
    }
    if !found {
        return Err(ServiceFileError::NotFound(format!(
            "No pg_service.conf found (user: {}, system: {}; PGSERVICEFILE / PGSYSCONFDIR or pg_config --sysconfdir)",
            user.map_or_else(|| "unavailable".into(), |path| path.display().to_string()),
            system.map_or_else(|| "unavailable".into(), |path| path.display().to_string()),
        )));
    }
    Ok(ServiceFileContents { entries, warning })
}

fn read_service_file(path: &Path, required: bool) -> Result<Option<String>, ServiceFileError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ServiceFileError::ReadAt {
            path: path.to_path_buf(),
            source: Arc::new(source),
        }),
    }
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

fn parse(content: &str, path: &Path) -> Vec<ServiceEntry> {
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
                source_path: path.to_path_buf(),
            });
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    // libpq uses the first matching section.
    let mut seen = std::collections::HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        seen.entry(entry.service_name.clone()).or_insert(i);
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

    #[test]
    fn empty_content_returns_no_entries() {
        assert_eq!(parse("", Path::new("/test")), Vec::new());
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
        let entries = parse(content, Path::new("/test"));
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
        let entries = parse(content, Path::new("/test"));
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
        let entries = parse(content, Path::new("/test"));
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
        let entries = parse(content, Path::new("/test"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_name, "mydb");
    }

    #[test]
    fn duplicate_sections_preserve_first_occurrence_order() {
        let content = "\
[mydb]
host=first.example.com
port=5432

[other]
host=other.example.com

[mydb]
host=second.example.com
port=5433
";
        let entries = parse(content, Path::new("/test"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].service_name, "mydb");
        assert_eq!(entries[1].service_name, "other");
    }

    #[test]
    fn section_with_no_keys() {
        let content = "\
[empty]
";
        let entries = parse(content, Path::new("/test"));
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
        let entries = parse(content, Path::new("/test"));
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
        let entries = parse(content, Path::new("/test"));
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
    fn user_services_override_duplicates_and_keep_system_only_entries() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let system = dir.path().join("system");
        std::fs::write(&user, "[shared]\nhost=user\n[user_only]\n").unwrap();
        std::fs::write(&system, "[shared]\nhost=system\n[system_only]\n").unwrap();

        let result = read_service_files(Some(&user), false, Some(&system)).unwrap();

        assert_eq!(
            result
                .entries
                .iter()
                .map(|e| e.service_name.as_str())
                .collect::<Vec<_>>(),
            ["shared", "user_only", "system_only"]
        );
        assert_eq!(result.entries[0].source_path, user);
        assert_eq!(result.entries[2].source_path, system);
        assert_eq!(result.entries[2].to_string(), "service=system_only");
        assert!(result.warning.is_none());
    }

    #[test]
    fn missing_default_user_still_lists_system_services() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("system");
        std::fs::write(&system, "[system_only]\n").unwrap();

        let result =
            read_service_files(Some(&dir.path().join("missing")), false, Some(&system)).unwrap();

        assert_eq!(result.entries[0].service_name, "system_only");
        assert_eq!(result.entries[0].source_path, system);
    }

    #[test]
    fn missing_explicit_user_does_not_fall_back_to_system() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let system = dir.path().join("system");
        std::fs::write(&system, "[system_only]\n").unwrap();

        let result = read_service_files(Some(&missing), true, Some(&system));

        assert!(matches!(result, Err(ServiceFileError::ReadAt {path, ..}) if path == missing));
    }

    #[test]
    fn absent_files_report_not_found() {
        let dir = tempfile::tempdir().unwrap();

        let result = read_service_files(
            Some(&dir.path().join("user")),
            false,
            Some(&dir.path().join("system")),
        );

        assert!(matches!(result, Err(ServiceFileError::NotFound(_))));
    }

    #[test]
    fn unreadable_user_reports_source_path_without_system_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        std::fs::write(&user, [0xff]).unwrap();

        let result = read_service_files(Some(&user), false, None);

        assert!(matches!(result, Err(ServiceFileError::ReadAt {path, ..}) if path == user));
    }

    #[test]
    fn unreadable_system_preserves_user_services_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let system = dir.path().join("system");
        std::fs::write(&user, "[user_only]\n").unwrap();
        std::fs::write(&system, [0xff]).unwrap();

        let result = read_service_files(Some(&user), false, Some(&system)).unwrap();

        assert_eq!(result.entries[0].service_name, "user_only");
        assert!(
            matches!(result.warning, Some(ServiceFileError::ReadAt {path, ..}) if path == system)
        );
    }

    #[test]
    fn environment_paths_replace_defaults_without_pg_config_lookup() {
        let (user, system) = service_file_paths(
            Some("/custom/user".into()),
            Some("/home/default".into()),
            Some("/custom/system".into()),
            || panic!("pg_config must not run"),
        );

        assert_eq!(user, Some("/custom/user".into()));
        assert_eq!(
            system,
            Some(PathBuf::from("/custom/system/pg_service.conf"))
        );
    }

    #[test]
    fn absent_environment_uses_platform_user_and_pg_config_directory() {
        let (user, system) = service_file_paths(None, Some("/home/default".into()), None, || {
            Some("/postgres/etc".into())
        });

        assert_eq!(user, Some("/home/default".into()));
        assert_eq!(system, Some(PathBuf::from("/postgres/etc/pg_service.conf")));
    }
    #[test]
    fn environment_overrides_load_both_files_through_reader() {
        const CHILD: &str = "SABIQL_CF06_SERVICE_READER_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let result = PgServiceFileReader::new().read_services().unwrap();
            assert_eq!(
                result
                    .entries
                    .iter()
                    .map(|e| e.service_name.as_str())
                    .collect::<Vec<_>>(),
                ["custom_user", "custom_system"]
            );
            assert_eq!(
                result.entries[0].source_path,
                PathBuf::from(std::env::var_os("PGSERVICEFILE").unwrap())
            );
            assert_eq!(
                result.entries[1].source_path,
                PathBuf::from(std::env::var_os("PGSYSCONFDIR").unwrap()).join("pg_service.conf")
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("custom.conf");
        std::fs::write(&user, "[custom_user]\n").unwrap();
        std::fs::write(dir.path().join("pg_service.conf"), "[custom_system]\n").unwrap();

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "adapters::postgres::pg_service::tests::environment_overrides_load_both_files_through_reader", "--nocapture"])
            .env(CHILD, "1")
            .env("PGSERVICEFILE", &user)
            .env("PGSYSCONFDIR", dir.path())
            .output().unwrap();

        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
