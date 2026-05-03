use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::ports::outbound::{AppSettings, SettingsStore, SettingsStoreError};
use crate::config::connection_config::{CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigFile};

const CONFIG_FILE_NAME: &str = "connections.toml";

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TomlSettingsStore {
    config_dir: PathBuf,
}

impl TomlSettingsStore {
    pub fn new() -> Result<Self, SettingsStoreError> {
        let config_dir = get_config_dir()?;
        Ok(Self { config_dir })
    }

    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    fn settings_file_path(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE_NAME)
    }

    fn load_config_file(
        &self,
        strict: bool,
    ) -> Result<Option<ConnectionConfigFile>, SettingsStoreError> {
        let path = self.settings_file_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let version_check = match toml::from_str::<ConfigVersionCheck>(&content) {
            Ok(version_check) => version_check,
            Err(e) if strict => return Err(e.into()),
            Err(_) => return Ok(None),
        };

        if version_check.version != CURRENT_VERSION {
            if strict {
                return Err(SettingsStoreError::VersionMismatch {
                    found: version_check.version,
                    expected: CURRENT_VERSION,
                });
            }
            return Ok(None);
        }

        match toml::from_str::<ConnectionConfigFile>(&content) {
            Ok(config) => Ok(Some(config)),
            Err(e) if strict => Err(e.into()),
            Err(_) => Ok(None),
        }
    }
}

impl SettingsStore for TomlSettingsStore {
    fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        Ok(self
            .load_config_file(false)?
            .map_or_else(AppSettings::default, |config| config.app_settings()))
    }

    fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)?;
        }

        let mut config = self
            .load_config_file(true)?
            .unwrap_or_else(|| ConnectionConfigFile {
                version: CURRENT_VERSION,
                theme: None,
                connections: vec![],
            });
        config.set_app_settings(settings);
        let content = toml::to_string_pretty(&config)?;
        let content_with_header = format!(
            "# sabiql connection configuration\n# WARNING: Passwords are stored in plain text\n\n{content}"
        );

        let path = self.settings_file_path();
        let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self.config_dir.join(format!(
            ".connections.toml.{}-{}.tmp",
            std::process::id(),
            counter,
        ));

        if let Err(e) = fs::write(&tmp_path, content_with_header) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e.into());
        }

        if let Err(e) = set_file_permissions(&tmp_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }

        if let Err(e) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e.into());
        }

        Ok(())
    }
}

fn get_config_dir() -> Result<PathBuf, SettingsStoreError> {
    let config_base = dirs::config_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find config directory",
        )
    })?;
    Ok(config_base.join("sabiql"))
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<(), SettingsStoreError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<(), SettingsStoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::theme_id::ThemeId;
    use tempfile::TempDir;

    #[test]
    fn missing_file_returns_default_settings() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
    }

    #[test]
    fn save_and_load_round_trips_theme() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        store
            .save(AppSettings {
                theme_id: ThemeId::Light,
            })
            .unwrap();

        let settings = store.load().unwrap();
        assert_eq!(settings.theme_id, ThemeId::Light);
    }

    #[test]
    fn save_preserves_existing_connections() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            r#"version = 2

[[connections]]
id = "test-id"
name = "Test"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "testpass"
ssl_mode = "prefer"
"#,
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        store
            .save(AppSettings {
                theme_id: ThemeId::Light,
            })
            .unwrap();

        let content = fs::read_to_string(temp_dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(content.contains("theme = \"light\""));
        assert!(content.contains("[[connections]]"));
        assert!(content.contains("name = \"Test\""));
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(CONFIG_FILE_NAME), "not = [").unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "version = 2\ntheme = \"terminal\"\nconnections = []\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
    }

    #[test]
    fn save_invalid_toml_returns_error_without_overwriting_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "not = [").unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let result = store.save(AppSettings {
            theme_id: ThemeId::Light,
        });

        assert!(matches!(
            result,
            Err(SettingsStoreError::TomlDeserialize(_))
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "not = [");
    }
}
