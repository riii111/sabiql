use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::ports::outbound::{AppSettings, SettingsStore, SettingsStoreError};
use crate::config::settings_config::SettingsConfigFile;

const SETTINGS_FILE_NAME: &str = "settings.toml";

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
        self.config_dir.join(SETTINGS_FILE_NAME)
    }
}

impl SettingsStore for TomlSettingsStore {
    fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        let path = self.settings_file_path();
        if !path.exists() {
            return Ok(AppSettings::default());
        }

        let content = fs::read_to_string(path)?;
        let Ok(config) = toml::from_str::<SettingsConfigFile>(&content) else {
            return Ok(AppSettings::default());
        };
        Ok(AppSettings::from(config))
    }

    fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)?;
        }

        let config = SettingsConfigFile::from(settings);
        let content = toml::to_string_pretty(&config)?;
        let content_with_header = format!("# sabiql settings\n\n{content}");

        let path = self.settings_file_path();
        let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self.config_dir.join(format!(
            ".settings.toml.{}-{}.tmp",
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
    fn invalid_toml_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(SETTINGS_FILE_NAME), "not = [").unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(SETTINGS_FILE_NAME),
            "version = 1\ntheme = \"terminal\"\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
    }
}
