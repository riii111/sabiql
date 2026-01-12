use std::fs;
use std::path::PathBuf;

use crate::app::ports::connection_store::{ConnectionStore, ConnectionStoreError};
use crate::domain::connection::{ConnectionId, ConnectionProfile};
use crate::infra::config::connection_config::{
    CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigFile,
};

const CONFIG_FILE_NAME: &str = "connections.toml";

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
        self.config_dir.join(CONFIG_FILE_NAME)
    }

    fn write_all(&self, profiles: &[ConnectionProfile]) -> Result<(), ConnectionStoreError> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)
                .map_err(|e| ConnectionStoreError::IoError(e.to_string()))?;
        }

        let config = ConnectionConfigFile::from_profiles(profiles);
        let content = toml::to_string_pretty(&config)
            .map_err(|e| ConnectionStoreError::WriteError(e.to_string()))?;

        let content_with_header = format!(
            "# sabiql connection configuration\n# WARNING: Passwords are stored in plain text\n\n{}",
            content
        );

        let path = self.config_file_path();
        fs::write(&path, content_with_header)
            .map_err(|e| ConnectionStoreError::WriteError(e.to_string()))?;

        set_file_permissions(&path)?;

        Ok(())
    }
}

impl ConnectionStore for TomlConnectionStore {
    fn load(&self) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        let profiles = self.load_all()?;
        Ok(profiles.into_iter().next())
    }

    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        let path = self.config_file_path();

        if !path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| ConnectionStoreError::ReadError(e.to_string()))?;

        // Check version first to detect v1 format before full parse fails
        let version_check: ConfigVersionCheck = toml::from_str(&content)
            .map_err(|e| ConnectionStoreError::InvalidFormat(e.to_string()))?;

        if version_check.version != CURRENT_VERSION {
            return Err(ConnectionStoreError::VersionMismatch {
                found: version_check.version,
                expected: CURRENT_VERSION,
            });
        }

        let config: ConnectionConfigFile = toml::from_str(&content)
            .map_err(|e| ConnectionStoreError::InvalidFormat(e.to_string()))?;

        config
            .to_profiles()
            .map_err(|e| ConnectionStoreError::InvalidFormat(e.to_string()))
    }

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        let mut profiles = match self.load_all() {
            Ok(p) => p,
            Err(ConnectionStoreError::VersionMismatch { .. }) => vec![],
            Err(e) => return Err(e),
        };

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
    let config_base = dirs::config_dir()
        .ok_or_else(|| ConnectionStoreError::IoError("Could not find config directory".into()))?;
    Ok(config_base.join("sabiql"))
}

#[cfg(unix)]
fn set_file_permissions(path: &std::path::Path) -> Result<(), ConnectionStoreError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms).map_err(|e| ConnectionStoreError::IoError(e.to_string()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &std::path::Path) -> Result<(), ConnectionStoreError> {
    Ok(())
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

    mod load_all {
        use super::*;

        #[test]
        fn returns_empty_vec_when_no_file_exists() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.load_all().unwrap();

            assert!(result.is_empty());
        }

        #[test]
        fn returns_version_mismatch_for_v1_format() {
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
        fn returns_error_for_invalid_toml() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            fs::write(&config_path, "invalid toml {{{{").unwrap();

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::InvalidFormat(_))
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

    mod find_by_id {
        use super::*;

        #[test]
        fn returns_some_when_found() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            let found = store.find_by_id(&profile.id).unwrap();

            assert!(found.is_some());
            assert_eq!(found.unwrap().name.as_str(), "Test");
        }

        #[test]
        fn returns_none_when_not_found() {
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
        fn returns_correct_path() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let path = store.storage_path();

            assert_eq!(path, temp_dir.path().join(CONFIG_FILE_NAME));
        }
    }
}
