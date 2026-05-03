use std::fs;
use std::path::PathBuf;

use super::app_config_file::{
    self, config_file_path, get_config_dir as app_config_dir, render_config_file, write_config_file,
};
use crate::app::ports::outbound::connection_store::{ConnectionStore, ConnectionStoreError};
use crate::config::connection_config::{CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigFile};
use crate::domain::connection::{ConnectionId, ConnectionProfile};

#[cfg(test)]
use super::app_config_file::CONFIG_FILE_NAME;
#[cfg(test)]
use std::path::Path;

pub struct TomlConnectionStore {
    config_dir: PathBuf,
}

impl TomlConnectionStore {
    pub fn new() -> Result<Self, ConnectionStoreError> {
        let config_dir = get_config_dir()?;
        Ok(Self { config_dir })
    }

    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    fn config_file_path(&self) -> PathBuf {
        config_file_path(&self.config_dir)
    }

    fn load_config_file(&self) -> Result<Option<ConnectionConfigFile>, ConnectionStoreError> {
        let path = self.config_file_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let version_check: ConfigVersionCheck = toml::from_str(&content)?;

        if version_check.version != CURRENT_VERSION {
            return Err(ConnectionStoreError::VersionMismatch {
                found: version_check.version,
                expected: CURRENT_VERSION,
            });
        }

        Ok(Some(toml::from_str::<ConnectionConfigFile>(&content)?))
    }

    fn write_all(&self, profiles: &[ConnectionProfile]) -> Result<(), ConnectionStoreError> {
        let mut config = ConnectionConfigFile::from(profiles);
        if let Some(existing_config) = self.load_config_file()? {
            config.theme = existing_config.theme;
        }
        let content = toml::to_string_pretty(&config)?;
        let content_with_header = render_config_file(&content);
        write_config_file(&self.config_dir, &content_with_header)?;

        Ok(())
    }
}

impl ConnectionStore for TomlConnectionStore {
    fn load(&self) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        let profiles = self.load_all()?;
        Ok(profiles.into_iter().next())
    }

    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        let Some(config) = self.load_config_file()? else {
            return Ok(vec![]);
        };

        Vec::<ConnectionProfile>::try_from(&config).map_err(ConnectionStoreError::InvalidProfile)
    }

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        let _guard = app_config_file::lock();
        let mut profiles = self.load_all()?;

        let normalized_name = profile.name.normalized();
        if profiles
            .iter()
            .any(|p| p.id != profile.id && p.name.normalized() == normalized_name)
        {
            return Err(ConnectionStoreError::DuplicateName(
                profile.name.as_str().to_string(),
            ));
        }

        if let Some(pos) = profiles.iter().position(|p| p.id == profile.id) {
            profiles[pos] = profile.clone();
        } else {
            profiles.push(profile.clone());
        }

        self.write_all(&profiles)
    }

    fn find_by_id(
        &self,
        id: &ConnectionId,
    ) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        let profiles = self.load_all()?;
        Ok(profiles.into_iter().find(|p| &p.id == id))
    }

    fn delete(&self, id: &ConnectionId) -> Result<(), ConnectionStoreError> {
        let _guard = app_config_file::lock();
        let mut profiles = self.load_all()?;
        let original_len = profiles.len();
        profiles.retain(|p| &p.id != id);

        if profiles.len() == original_len {
            return Err(ConnectionStoreError::NotFound(id.to_string()));
        }

        self.write_all(&profiles)
    }

    fn storage_path(&self) -> PathBuf {
        self.config_file_path()
    }
}

fn get_config_dir() -> Result<PathBuf, ConnectionStoreError> {
    Ok(app_config_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::SslMode;
    use tempfile::TempDir;

    fn make_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new(
            name,
            "localhost",
            5432,
            "testdb",
            "testuser",
            "testpass",
            SslMode::Prefer,
        )
        .unwrap()
    }

    mod loading {
        use super::*;

        #[test]
        fn no_file_returns_empty_vec() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.load_all().unwrap();

            assert!(result.is_empty());
        }

        #[test]
        fn reports_version_mismatch_for_v1_format() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let content = r#"
version = 1

[connection]
id = "test-id"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "testpass"
ssl_mode = "prefer"
"#;
            fs::write(&config_path, content).unwrap();

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::VersionMismatch {
                    found: 1,
                    expected: 2
                })
            ));
        }

        #[test]
        fn reports_error_for_invalid_toml() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            fs::write(&config_path, "invalid toml {{{{").unwrap();

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::TomlDeserialize(_))
            ));
        }
    }

    mod save {
        use super::*;

        #[test]
        fn creates_config_directory_if_missing() {
            let temp_dir = TempDir::new().unwrap();
            let config_dir = temp_dir.path().join("nested").join("config");
            let store = TomlConnectionStore::with_config_dir(config_dir.clone());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            assert!(config_dir.exists());
            assert!(store.storage_path().exists());
        }

        #[test]
        fn duplicate_name_returns_error() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile1 = make_test_profile("Production");
            let profile2 = make_test_profile("production"); // case-insensitive match

            store.save(&profile1).unwrap();
            let result = store.save(&profile2);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::DuplicateName(_))
            ));
        }

        #[test]
        fn same_id_updates_without_duplicate_error() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let mut profile = make_test_profile("Production");
            store.save(&profile).unwrap();

            profile.host = "newhost".to_string();
            let result = store.save(&profile);

            assert!(result.is_ok());
        }

        #[test]
        fn preserves_existing_theme() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                "version = 2\ntheme = \"light\"\nconnections = []\n",
            )
            .unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            let content = fs::read_to_string(config_path).unwrap();
            assert!(content.contains("theme = \"light\""));
            assert!(content.contains("[[connections]]"));
        }

        #[cfg(unix)]
        #[test]
        fn sets_permissions_to_0600() {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            let path = store.storage_path();
            let metadata = fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    mod delete {
        use super::*;

        #[test]
        fn removes_connection_by_id() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            store.delete(&profile.id).unwrap();

            assert!(store.load_all().unwrap().is_empty());
        }

        #[test]
        fn nonexistent_id_returns_not_found() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.delete(&ConnectionId::new());

            assert!(matches!(result, Err(ConnectionStoreError::NotFound(_))));
        }
    }

    mod lookup {
        use super::*;

        #[test]
        fn existing_id_finds_connection() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            let found = store.find_by_id(&profile.id).unwrap();

            assert!(found.is_some());
            assert_eq!(found.unwrap().name.as_str(), "Test");
        }

        #[test]
        fn missing_id_returns_none() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let found = store.find_by_id(&ConnectionId::new()).unwrap();

            assert!(found.is_none());
        }
    }

    mod roundtrip {
        use super::*;

        #[test]
        fn save_and_load_preserves_data() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test Connection");

            store.save(&profile).unwrap();
            let loaded = store.load().unwrap();

            assert!(loaded.is_some());
            let loaded = loaded.unwrap();
            assert_eq!(loaded.name.as_str(), profile.name.as_str());
            assert_eq!(loaded.host, profile.host);
            assert_eq!(loaded.port, profile.port);
            assert_eq!(loaded.database, profile.database);
            assert_eq!(loaded.username, profile.username);
            assert_eq!(loaded.password, profile.password);
            assert_eq!(loaded.ssl_mode, profile.ssl_mode);
        }
    }

    mod storage_path {
        use super::*;

        #[test]
        fn matches_config_file_path() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let path = store.storage_path();

            assert_eq!(path, temp_dir.path().join(CONFIG_FILE_NAME));
        }
    }

    mod version_mismatch {
        use super::*;

        #[test]
        fn save_returns_error_instead_of_losing_data() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let v1_content = r#"
version = 1

[connection]
id = "test-id"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "testpass"
ssl_mode = "prefer"
"#;
            fs::write(&config_path, v1_content).unwrap();

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("New Connection");
            let result = store.save(&profile);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::VersionMismatch {
                    found: 1,
                    expected: 2
                })
            ));

            let content_after = fs::read_to_string(&config_path).unwrap();
            assert!(content_after.contains("version = 1"));
        }
    }

    mod atomic_write {
        use super::*;

        #[test]
        fn leaves_no_temp_file() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            let tmp_files: Vec<_> = fs::read_dir(temp_dir.path())
                .unwrap()
                .flatten()
                .filter(|e| {
                    e.file_name().to_str().is_some_and(|n| {
                        Path::new(n)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
                    })
                })
                .collect();
            assert!(tmp_files.is_empty());
        }

        #[test]
        fn existing_file_preserved_on_save_roundtrip() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile1 = make_test_profile("First");
            let mut profile2 = make_test_profile("Second");
            store.save(&profile1).unwrap();
            store.save(&profile2).unwrap();

            profile2.host = "updated-host".to_string();
            store.save(&profile2).unwrap();

            let all = store.load_all().unwrap();
            assert_eq!(all.len(), 2);
            assert!(all.iter().any(|p| p.name.as_str() == "First"));
            assert!(
                all.iter()
                    .any(|p| p.name.as_str() == "Second" && p.host == "updated-host")
            );
        }
    }
}
