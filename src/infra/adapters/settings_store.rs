use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use super::clipboard::{ArboardClipboard, Osc52Clipboard};

use super::app_config_file::{
    self, config_file_path, get_config_dir as app_config_dir, render_config_file, write_config_file,
};
use crate::app::model::shared::settings::KeymapPreset;
use crate::app::model::shared::theme_id::ThemeId;
use crate::app::ports::outbound::{
    AppSettings, ClipboardWriter, SettingsStore, SettingsStoreError,
};
use crate::config::{
    CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigFile, is_supported_config_version,
};

pub struct TomlSettingsStore {
    config_dir: PathBuf,
}

impl TomlSettingsStore {
    pub fn new() -> Result<Self, SettingsStoreError> {
        let config_dir = app_config_dir()?;
        Ok(Self { config_dir })
    }

    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        Ok(self
            .load_config_file_lenient()?
            .map_or_else(AppSettings::default, app_settings))
    }

    pub fn load_clipboard(&self) -> Result<Arc<dyn ClipboardWriter>, SettingsStoreError> {
        let config = self.load_config_file_lenient()?;
        match config
            .as_ref()
            .and_then(|config| config.clipboard_backend.as_deref())
        {
            None | Some("native") => Ok(Arc::new(ArboardClipboard)),
            Some("osc52") => Ok(Arc::new(Osc52Clipboard)),
            Some(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "clipboard_backend must be native or osc52",
            )
            .into()),
        }
    }

    fn load_config_file_lenient(&self) -> Result<Option<ConnectionConfigFile>, SettingsStoreError> {
        match self.load_config_file_strict() {
            Err(
                SettingsStoreError::TomlDeserialize(_) | SettingsStoreError::VersionMismatch { .. },
            ) => Ok(None),
            result => result,
        }
    }

    fn load_config_file_strict(&self) -> Result<Option<ConnectionConfigFile>, SettingsStoreError> {
        let path = config_file_path(&self.config_dir);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let version_check: ConfigVersionCheck = toml::from_str(&content)?;

        if !is_supported_config_version(version_check.version) {
            return Err(SettingsStoreError::VersionMismatch {
                found: version_check.version,
                expected: CURRENT_VERSION,
            });
        }

        Ok(Some(toml::from_str::<ConnectionConfigFile>(&content)?))
    }
}

impl SettingsStore for TomlSettingsStore {
    fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
        let _guard = app_config_file::lock();

        let mut config = self.load_config_file_strict()?.unwrap_or_default();
        config.version = CURRENT_VERSION;
        set_app_settings(&mut config, settings);
        let content = toml::to_string_pretty(&config)?;
        let content_with_header = render_config_file(&content);
        write_config_file(&self.config_dir, &content_with_header)?;

        Ok(())
    }
}

fn app_settings(config: ConnectionConfigFile) -> AppSettings {
    AppSettings {
        theme_id: config
            .theme
            .as_deref()
            .and_then(ThemeId::from_config_value)
            .unwrap_or_default(),
        keymap_preset: config
            .keymap_preset
            .as_deref()
            .and_then(KeymapPreset::from_config_value)
            .unwrap_or(KeymapPreset::Default),
        er_browser: config.er_browser,
    }
}

fn set_app_settings(config: &mut ConnectionConfigFile, settings: AppSettings) {
    config.theme = Some(settings.theme_id.config_value().to_string());
    config.keymap_preset = Some(settings.keymap_preset.config_value().to_string());
    config.er_browser = settings.er_browser;
}

#[cfg(test)]
mod tests {
    use super::app_config_file::CONFIG_FILE_NAME;

    use super::*;
    use tempfile::TempDir;

    #[rstest::rstest]
    #[case(2, "")]
    #[case(3, "")]
    #[case(3, "clipboard_backend = \"native\"\n")]
    #[case(3, "clipboard_backend = \"osc52\"\n")]
    fn loads_clipboard_with_legacy_or_explicit_configuration(
        #[case] version: u32,
        #[case] backend: &str,
    ) {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE_NAME),
            format!("version = {version}\n{backend}connections = []\n"),
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(dir.path().to_path_buf());

        assert!(store.load_clipboard().is_ok());
    }

    #[test]
    fn unknown_clipboard_backend_is_rejected() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE_NAME),
            "version = 3\nclipboard_backend = \"auto\"\nconnections = []\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(dir.path().to_path_buf());

        assert!(store.load_clipboard().is_err());
    }

    #[test]
    fn saving_ui_settings_preserves_explicit_clipboard_backend() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE_NAME),
            "version = 3\nclipboard_backend = \"osc52\"\nconnections = []\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(dir.path().to_path_buf());

        store.save(AppSettings::default()).unwrap();

        let content = fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        let config: ConnectionConfigFile = toml::from_str(&content).unwrap();
        assert_eq!(config.clipboard_backend.as_deref(), Some("osc52"));
    }

    #[test]
    fn saving_ui_settings_preserves_password_storage_fields() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE_NAME),
            r#"version = 3

[[connections]]
id = "legacy"
name = "Legacy"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "legacy-password"
ssl_mode = "prefer"

[[connections]]
id = "managed"
name = "Managed"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password_ref = "connection:managed"
ssl_mode = "prefer"
"#,
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(dir.path().to_path_buf());

        store.save(AppSettings::default()).unwrap();

        let content = fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        let config: ConnectionConfigFile = toml::from_str(&content).unwrap();
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(
            config.connections[0].password.as_deref(),
            Some("legacy-password")
        );
        assert_eq!(config.connections[0].password_ref, None);
        assert_eq!(
            config.connections[1].password_ref.as_deref(),
            Some("connection:managed")
        );
        assert_eq!(config.connections[1].password, None);
    }

    #[test]
    fn missing_file_returns_default_settings() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn save_and_load_round_trips_settings() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        store
            .save(AppSettings {
                theme_id: ThemeId::Light,
                keymap_preset: KeymapPreset::Ide,
                er_browser: Some("Google Chrome".to_string()),
            })
            .unwrap();

        let settings = store.load().unwrap();
        assert_eq!(settings.theme_id, ThemeId::Light);
        assert_eq!(settings.keymap_preset, KeymapPreset::Ide);
        assert_eq!(settings.er_browser.as_deref(), Some("Google Chrome"));
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
                keymap_preset: KeymapPreset::Ide,
                er_browser: Some("Firefox".to_string()),
            })
            .unwrap();

        let content = fs::read_to_string(temp_dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(content.contains("theme = \"light\""));
        assert!(content.contains("keymap_preset = \"ide\""));
        assert!(content.contains("er_browser = \"Firefox\""));
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
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
        assert_eq!(settings.er_browser, None);
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
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn missing_theme_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "version = 2\nconnections = []\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.theme_id, ThemeId::Default);
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn unknown_keymap_preset_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "version = 2\nkeymap_preset = \"vim\"\nconnections = []\n",
        )
        .unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load().unwrap();

        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn save_invalid_toml_returns_error_without_overwriting_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "not = [").unwrap();
        let store = TomlSettingsStore::with_config_dir(temp_dir.path().to_path_buf());

        let result = store.save(AppSettings {
            theme_id: ThemeId::Light,
            keymap_preset: KeymapPreset::Default,
            er_browser: None,
        });

        assert!(matches!(
            result,
            Err(SettingsStoreError::TomlDeserialize(_))
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "not = [");
    }
}
