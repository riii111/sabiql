use std::fs;
use std::path::PathBuf;

use crate::app::ports::connection_store::{ConnectionStore, ConnectionStoreError};
use crate::domain::connection::ConnectionProfile;
use crate::infra::config::connection_config::{CURRENT_VERSION, ConnectionConfigFile};

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
}

impl Default for TomlConnectionStore {
    fn default() -> Self {
        Self::new().expect("Failed to initialize connection store")
    }
}

impl ConnectionStore for TomlConnectionStore {
    fn load(&self) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        let path = self.config_file_path();

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| ConnectionStoreError::ReadError(e.to_string()))?;

        let config: ConnectionConfigFile = toml::from_str(&content)
            .map_err(|e| ConnectionStoreError::InvalidFormat(e.to_string()))?;

        if config.version != CURRENT_VERSION {
            return Err(ConnectionStoreError::VersionMismatch {
                found: config.version,
                expected: CURRENT_VERSION,
            });
        }

        Ok(Some(config.to_profile()))
    }

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)
                .map_err(|e| ConnectionStoreError::IoError(e.to_string()))?;
        }

        let config = ConnectionConfigFile::from_profile(profile);
        let content = toml::to_string_pretty(&config)
            .map_err(|e| ConnectionStoreError::WriteError(e.to_string()))?;

        let content_with_header = format!(
            "# sabiql connection configuration\n# WARNING: Password is stored in plain text\n\n{}",
            content
        );

        let path = self.config_file_path();
        fs::write(&path, content_with_header)
            .map_err(|e| ConnectionStoreError::WriteError(e.to_string()))?;

        set_file_permissions(&path)?;

        Ok(())
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

    fn make_test_profile() -> ConnectionProfile {
        ConnectionProfile::new(
            "localhost",
            5432,
            "testdb",
            "testuser",
            "testpass",
            SslMode::Prefer,
        )
    }

    mod load {
        use super::*;

        #[test]
        fn returns_none_when_no_file_exists() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.load().unwrap();

            assert!(result.is_none());
        }

        #[test]
        fn returns_version_mismatch_for_old_version() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let content = r#"
version = 0

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
            let result = store.load();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::VersionMismatch {
                    found: 0,
                    expected: 1
                })
            ));
        }

        #[test]
        fn returns_error_for_invalid_toml() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            fs::write(&config_path, "invalid toml {{{{").unwrap();

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load();

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
            let profile = make_test_profile();

            store.save(&profile).unwrap();

            assert!(config_dir.exists());
            assert!(store.storage_path().exists());
        }

        #[cfg(unix)]
        #[test]
        fn sets_permissions_to_0600() {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile();

            store.save(&profile).unwrap();

            let path = store.storage_path();
            let metadata = fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    mod roundtrip {
        use super::*;

        #[test]
        fn save_and_load_preserves_data() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile();

            store.save(&profile).unwrap();
            let loaded = store.load().unwrap();

            assert!(loaded.is_some());
            let loaded = loaded.unwrap();
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
