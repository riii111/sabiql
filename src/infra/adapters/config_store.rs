use std::fs;
use std::path::PathBuf;

use super::app_config_file::{
    self, config_file_path, get_config_dir as app_config_dir, render_config_file, write_config_file,
};
use crate::app::model::shared::settings::{IconMode, KeymapPreset};
use crate::app::model::shared::theme_id::ThemeId;
use crate::app::ports::outbound::connection_store::{ConnectionStore, ConnectionStoreError};
use crate::app::ports::outbound::{AppSettings, SettingsStore, SettingsStoreError};
use crate::config::{
    CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigFile, is_supported_config_version,
};
use crate::domain::connection::{ConnectionId, ConnectionProfile};

pub struct TomlConfigStore {
    config_dir: PathBuf,
}

trait ConfigFileError: From<std::io::Error> + From<toml::de::Error> + From<toml::ser::Error> {
    fn version_mismatch(found: u32) -> Self;
}

impl ConfigFileError for ConnectionStoreError {
    fn version_mismatch(found: u32) -> Self {
        Self::VersionMismatch {
            found,
            expected: CURRENT_VERSION,
        }
    }
}

impl ConfigFileError for SettingsStoreError {
    fn version_mismatch(found: u32) -> Self {
        Self::VersionMismatch {
            found,
            expected: CURRENT_VERSION,
        }
    }
}

impl TomlConfigStore {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self::with_config_dir(app_config_dir()?))
    }

    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn storage_path(&self) -> PathBuf {
        config_file_path(&self.config_dir)
    }

    pub fn load_settings(&self) -> AppSettings {
        // Unreadable settings fall back to defaults so a broken file still lets the app start.
        self.read::<SettingsStoreError>()
            .ok()
            .flatten()
            .map_or_else(AppSettings::default, app_settings)
    }

    fn read<E: ConfigFileError>(&self) -> Result<Option<ConnectionConfigFile>, E> {
        let path = self.storage_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let version_check: ConfigVersionCheck = toml::from_str(&content)?;
        if !is_supported_config_version(version_check.version) {
            return Err(E::version_mismatch(version_check.version));
        }

        Ok(Some(toml::from_str(&content)?))
    }

    fn update<E: ConfigFileError>(
        &self,
        apply: impl FnOnce(&mut ConnectionConfigFile) -> Result<(), E>,
    ) -> Result<(), E> {
        let _guard = app_config_file::lock();
        let mut config = self.read::<E>()?.unwrap_or_default();
        apply(&mut config)?;
        let content = toml::to_string_pretty(&config)?;
        write_config_file(&self.config_dir, &render_config_file(&content))?;
        Ok(())
    }

    fn update_profiles(
        &self,
        apply: impl FnOnce(&mut Vec<ConnectionProfile>) -> Result<(), ConnectionStoreError>,
    ) -> Result<(), ConnectionStoreError> {
        self.update(|config| {
            let mut profiles = config
                .profiles()
                .map_err(ConnectionStoreError::InvalidProfile)?;
            apply(&mut profiles)?;
            config.set_profiles(&profiles);
            Ok(())
        })
    }
}

impl ConnectionStore for TomlConfigStore {
    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        let Some(config) = self.read::<ConnectionStoreError>()? else {
            return Ok(vec![]);
        };

        config
            .profiles()
            .map_err(ConnectionStoreError::InvalidProfile)
    }

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        self.update_profiles(|profiles| {
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
            Ok(())
        })
    }

    fn find_by_id(
        &self,
        id: &ConnectionId,
    ) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        let profiles = self.load_all()?;
        Ok(profiles.into_iter().find(|p| &p.id == id))
    }

    fn delete(&self, id: &ConnectionId) -> Result<(), ConnectionStoreError> {
        self.update_profiles(|profiles| {
            let original_len = profiles.len();
            profiles.retain(|p| &p.id != id);
            if profiles.len() == original_len {
                return Err(ConnectionStoreError::NotFound(id.to_string()));
            }
            Ok(())
        })
    }
}

impl SettingsStore for TomlConfigStore {
    fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
        self.update(|config| {
            config.icon_mode = Some(settings.icon_mode.config_value().to_owned());
            config.theme = Some(settings.theme_id.config_value().to_string());
            config.keymap_preset = Some(settings.keymap_preset.config_value().to_string());
            config.er_browser = settings.er_browser;
            Ok(())
        })
    }
}

fn app_settings(config: ConnectionConfigFile) -> AppSettings {
    AppSettings {
        icon_mode: config
            .icon_mode
            .as_deref()
            .and_then(IconMode::from_config_value)
            .unwrap_or_default(),
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

#[cfg(test)]
mod connection_tests {
    use std::fs;

    use super::TomlConfigStore;
    use super::app_config_file::CONFIG_FILE_NAME;
    use crate::app::ports::outbound::connection_store::{ConnectionStore, ConnectionStoreError};
    use crate::domain::connection::SslMode;
    use crate::domain::connection::{ConnectionConfig, DatabaseType, PostgresConnectionConfig};
    use crate::domain::connection::{ConnectionId, ConnectionProfile};
    use std::path::Path;
    use tempfile::TempDir;

    fn make_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new_postgres(
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
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

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

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::VersionMismatch {
                    found: 1,
                    expected: 3
                })
            ));
        }

        #[test]
        fn reports_error_for_invalid_toml() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            fs::write(&config_path, "invalid toml {{{{").unwrap();

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::TomlDeserialize(_))
            ));
        }

        #[test]
        fn loads_v2_entries_as_postgresql() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let content = r#"
version = 2

[[connections]]
id = "test-id"
name = "Production"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "testpass"
ssl_mode = "prefer"
"#;
            fs::write(&config_path, content).unwrap();

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let profiles = store.load_all().unwrap();

            assert_eq!(profiles.len(), 1);
            assert_eq!(profiles[0].database_type(), DatabaseType::PostgreSQL);
            assert_eq!(profiles[0].config.as_postgres().unwrap().database, "testdb");
            assert!(
                fs::read_to_string(&config_path)
                    .unwrap()
                    .contains("version = 2")
            );
        }

        #[test]
        fn missing_username_and_blank_host_load_as_empty_strings() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let content = r#"
version = 2

[[connections]]
id = "test-id"
name = "Local"
host = ""
port = 5432
database = "testdb"
password = ""
ssl_mode = "prefer"
"#;
            fs::write(&config_path, content).unwrap();

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let profiles = store.load_all().unwrap();

            assert_eq!(profiles.len(), 1);
            let config = profiles[0].config.as_postgres().unwrap();
            assert_eq!(config.username, "");
            assert_eq!(config.host, "");
        }
    }

    mod save {
        use super::*;

        #[test]
        fn creates_config_directory_if_missing() {
            let temp_dir = TempDir::new().unwrap();
            let config_dir = temp_dir.path().join("nested").join("config");
            let store = TomlConfigStore::with_config_dir(config_dir.clone());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            assert!(config_dir.exists());
            assert!(store.storage_path().exists());
        }

        #[test]
        fn duplicate_name_returns_error() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

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
        fn preserves_existing_app_settings() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                "version = 2\ntheme = \"light\"\nicon_mode = \"unicode\"\nkeymap_preset = \"ide\"\ner_browser = \"Firefox\"\nconnections = []\n",
            )
            .unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            let content = fs::read_to_string(config_path).unwrap();
            assert!(content.contains("theme = \"light\""));
            assert!(content.contains("icon_mode = \"unicode\""));
            assert!(content.contains("keymap_preset = \"ide\""));
            assert!(content.contains("er_browser = \"Firefox\""));
            assert!(content.contains("[[connections]]"));
        }

        #[cfg(unix)]
        #[test]
        fn sets_permissions_to_0600() {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
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
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            store.delete(&profile.id).unwrap();

            assert!(store.load_all().unwrap().is_empty());
        }

        #[test]
        fn nonexistent_id_returns_not_found() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.delete(&ConnectionId::new());

            assert!(matches!(result, Err(ConnectionStoreError::NotFound(_))));
        }
    }

    mod lookup {
        use super::*;

        #[test]
        fn existing_id_finds_connection() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            let found = store.find_by_id(&profile.id).unwrap();

            assert!(found.is_some());
            assert_eq!(found.unwrap().name.as_str(), "Test");
        }

        #[test]
        fn missing_id_returns_none() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

            let found = store.find_by_id(&ConnectionId::new()).unwrap();

            assert!(found.is_none());
        }
    }

    mod roundtrip {
        use super::*;

        #[test]
        fn empty_sqlite_path_returns_invalid_profile() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);

            let content = r#"
version = 3

[[connections]]
id = "sqlite-id"
name = "Local"
db_type = "sqlite"
path = ""
"#;
            fs::write(&config_path, content).unwrap();

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::InvalidProfile(_))
            ));
        }
    }

    mod storage_path {
        use super::*;

        #[test]
        fn matches_config_file_path() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

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

            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("New Connection");
            let result = store.save(&profile);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::VersionMismatch {
                    found: 1,
                    expected: 3
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
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());
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
        fn updating_one_profile_preserves_the_other_on_disk() {
            let temp_dir = TempDir::new().unwrap();
            let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

            let profile1 = make_test_profile("First");
            let mut profile2 = make_test_profile("Second");
            store.save(&profile1).unwrap();
            store.save(&profile2).unwrap();

            profile2.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "updated-host",
                5432,
                "testdb",
                "testuser",
                "testpass",
                SslMode::Prefer,
            ));
            store.save(&profile2).unwrap();

            let all = store.load_all().unwrap();
            assert_eq!(all.len(), 2);
            assert!(all.contains(&profile1));
            assert!(all.contains(&profile2));
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use std::fs;

    use super::TomlConfigStore;
    use super::app_config_file::CONFIG_FILE_NAME;
    use crate::app::model::shared::settings::{IconMode, KeymapPreset};
    use crate::app::model::shared::theme_id::ThemeId;
    use crate::app::ports::outbound::{AppSettings, SettingsStore, SettingsStoreError};
    use tempfile::TempDir;

    #[test]
    fn missing_file_returns_default_settings() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load_settings();

        assert_eq!(settings.icon_mode, IconMode::Unicode);
        assert_eq!(settings.theme_id, ThemeId::Default);
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn save_and_load_round_trips_settings() {
        let temp_dir = TempDir::new().unwrap();
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        store
            .save(AppSettings {
                theme_id: ThemeId::Light,
                icon_mode: IconMode::NerdFont,
                keymap_preset: KeymapPreset::Ide,
                er_browser: Some("Google Chrome".to_string()),
            })
            .unwrap();

        let settings = store.load_settings();
        assert_eq!(settings.icon_mode, IconMode::NerdFont);
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
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        store
            .save(AppSettings {
                theme_id: ThemeId::Light,
                icon_mode: IconMode::NerdFont,
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
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load_settings();

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
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load_settings();

        assert_eq!(settings.theme_id, ThemeId::Default);
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn missing_display_settings_use_defaults() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "version = 2\nconnections = []\n",
        )
        .unwrap();
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load_settings();

        assert_eq!(settings.icon_mode, IconMode::Unicode);
        assert_eq!(settings.theme_id, ThemeId::Default);
        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn configured_icon_modes_override_the_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        let store = TomlConfigStore::with_config_dir(dir.path().to_owned());

        for mode in IconMode::ALL {
            let value = mode.config_value();
            fs::write(
                &path,
                format!("version = 2\nconnections = []\nicon_mode = \"{value}\"\n"),
            )
            .unwrap();

            assert_eq!(store.load_settings().icon_mode, mode);
        }
    }

    #[test]
    fn unknown_keymap_preset_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(CONFIG_FILE_NAME),
            "version = 2\nkeymap_preset = \"vim\"\nconnections = []\n",
        )
        .unwrap();
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let settings = store.load_settings();

        assert_eq!(settings.keymap_preset, KeymapPreset::Default);
    }

    #[test]
    fn save_invalid_toml_returns_error_without_overwriting_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "not = [").unwrap();
        let store = TomlConfigStore::with_config_dir(temp_dir.path().to_path_buf());

        let result = store.save(AppSettings {
            theme_id: ThemeId::Light,
            icon_mode: IconMode::NerdFont,
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
