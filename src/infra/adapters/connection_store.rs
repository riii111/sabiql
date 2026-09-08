use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use super::app_config_file::{
    self, config_file_path, get_config_dir as app_config_dir, render_config_file, write_config_file,
};
use crate::app::ports::outbound::SecretStore;
use crate::app::ports::outbound::connection_store::{ConnectionStore, ConnectionStoreError};
use crate::config::{
    CURRENT_VERSION, ConfigVersionCheck, ConnectionConfigEntry, ConnectionConfigFile,
    is_supported_config_version,
};
use crate::domain::connection::{ConnectionConfig, ConnectionId, ConnectionProfile, DatabaseType};
use uuid::Uuid;

use super::PlatformSecretStore;

pub struct TomlConnectionStore {
    config_dir: PathBuf,
    secret_store: Arc<dyn SecretStore>,
}

impl TomlConnectionStore {
    pub fn new() -> Result<Self, ConnectionStoreError> {
        let config_dir = app_config_dir()?;
        Ok(Self {
            config_dir,
            secret_store: Arc::new(PlatformSecretStore::new()),
        })
    }

    #[cfg(test)]
    fn with_config_dir_and_secret_store(
        config_dir: PathBuf,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            config_dir,
            secret_store,
        }
    }

    #[cfg(test)]
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self::with_config_dir_and_secret_store(
            config_dir,
            Arc::new(tests::TestSecretStore::default()),
        )
    }

    pub fn storage_path(&self) -> PathBuf {
        config_file_path(&self.config_dir)
    }

    fn load_config_file(&self) -> Result<Option<ConnectionConfigFile>, ConnectionStoreError> {
        let path = config_file_path(&self.config_dir);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let version_check: ConfigVersionCheck = toml::from_str(&content)?;

        if !is_supported_config_version(version_check.version) {
            return Err(ConnectionStoreError::VersionMismatch {
                found: version_check.version,
                expected: CURRENT_VERSION,
            });
        }

        Ok(Some(toml::from_str::<ConnectionConfigFile>(&content)?))
    }

    fn write_config(&self, config: &ConnectionConfigFile) -> Result<(), ConnectionStoreError> {
        let content = toml::to_string_pretty(&config)?;
        let content_with_header = render_config_file(&content);
        write_config_file(&self.config_dir, &content_with_header)?;

        Ok(())
    }

    fn load_profiles(
        &self,
        config: &ConnectionConfigFile,
    ) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        validate_password_refs(config)?;
        config
            .connections
            .iter()
            .map(|entry| {
                let password = self.password_for_entry(entry)?;
                entry
                    .to_profile_with_password(password)
                    .map_err(ConnectionStoreError::InvalidProfile)
            })
            .collect()
    }

    fn password_for_entry(
        &self,
        entry: &ConnectionConfigEntry,
    ) -> Result<String, ConnectionStoreError> {
        if entry.db_type == DatabaseType::SQLite {
            return Ok(String::new());
        }
        entry.password_ref.as_deref().map_or_else(
            || Ok(entry.password.clone().unwrap_or_default()),
            |reference| self.secret_store.get(reference).map_err(Into::into),
        )
    }

    fn empty_config() -> ConnectionConfigFile {
        ConnectionConfigFile {
            version: CURRENT_VERSION,
            theme: None,
            keymap_preset: None,
            er_browser: None,
            clipboard_backend: None,
            connections: vec![],
        }
    }

    fn password(profile: &ConnectionProfile) -> Option<&str> {
        match &profile.config {
            ConnectionConfig::PostgreSQL(config) => Some(config.password.as_str()),
            ConnectionConfig::MySQL(config) => Some(config.password.as_str()),
            ConnectionConfig::SQLite(_) => None,
        }
    }

    fn password_ref(profile: &ConnectionProfile) -> String {
        format!("connection:{}", profile.id)
    }

    fn replacement_password_ref(profile: &ConnectionProfile) -> String {
        format!("{}:{}", Self::password_ref(profile), Uuid::new_v4())
    }
}

impl ConnectionStore for TomlConnectionStore {
    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        let _guard = app_config_file::lock();
        let Some(config) = self.load_config_file()? else {
            return Ok(vec![]);
        };

        self.load_profiles(&config)
    }

    fn save(&self, profile: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        let _guard = app_config_file::lock();
        let mut config = self.load_config_file()?.unwrap_or_else(Self::empty_config);
        let profiles = self.load_profiles(&config)?;

        let normalized_name = profile.name.normalized();
        if profiles
            .iter()
            .any(|p| p.id != profile.id && p.name.normalized() == normalized_name)
        {
            return Err(ConnectionStoreError::DuplicateName(
                profile.name.as_str().to_string(),
            ));
        }

        let existing = config
            .connections
            .iter()
            .find(|entry| entry.id == profile.id.as_str())
            .cloned();
        let old_ref = existing
            .as_ref()
            .and_then(|entry| entry.password_ref.clone());
        let password = Self::password(profile).filter(|password| !password.is_empty());

        if let Some(password) = password {
            let reference = old_ref.as_ref().map_or_else(
                || Self::password_ref(profile),
                |_| Self::replacement_password_ref(profile),
            );
            self.secret_store.set(&reference, password)?;
            let entry = ConnectionConfigEntry::from_profile_with_password_ref(
                profile,
                Some(reference.clone()),
            );
            let previous_config = config.clone();
            replace_entry(&mut config.connections, entry);
            if let Err(error) = self.write_config(&config) {
                let _ = self.secret_store.delete(&reference);
                return Err(error);
            }
            if let Some(old_ref) = old_ref
                && let Err(error) = self.secret_store.delete(&old_ref)
            {
                let _ = self.write_config(&previous_config);
                let _ = self.secret_store.delete(&reference);
                return Err(error.into());
            }
        } else if let Some(reference) = old_ref {
            let previous_config = config.clone();
            let entry = ConnectionConfigEntry::from_profile_with_password_ref(profile, None);
            replace_entry(&mut config.connections, entry);
            self.write_config(&config)?;
            if let Err(error) = self.secret_store.delete(&reference) {
                let _ = self.write_config(&previous_config);
                return Err(error.into());
            }
        } else {
            let entry = ConnectionConfigEntry::from_profile_with_password_ref(profile, None);
            replace_entry(&mut config.connections, entry);
            self.write_config(&config)?;
        }
        Ok(())
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
        let mut config = self
            .load_config_file()?
            .ok_or_else(|| ConnectionStoreError::NotFound(id.to_string()))?;
        self.load_profiles(&config)?;
        let Some(entry_index) = config
            .connections
            .iter()
            .position(|entry| entry.id == id.as_str())
        else {
            return Err(ConnectionStoreError::NotFound(id.to_string()));
        };

        let old_ref = config.connections[entry_index].password_ref.clone();
        let previous_config = config.clone();
        config.connections.remove(entry_index);
        self.write_config(&config)?;
        if let Some(reference) = old_ref
            && let Err(error) = self.secret_store.delete(&reference)
        {
            let _ = self.write_config(&previous_config);
            return Err(error.into());
        }

        Ok(())
    }
}

fn replace_entry(entries: &mut Vec<ConnectionConfigEntry>, entry: ConnectionConfigEntry) {
    if let Some(existing) = entries.iter_mut().find(|existing| existing.id == entry.id) {
        *existing = entry;
    } else {
        entries.push(entry);
    }
}

fn validate_password_refs(config: &ConnectionConfigFile) -> Result<(), ConnectionStoreError> {
    let mut references = HashSet::new();
    for entry in &config.connections {
        let Some(reference) = entry.password_ref.as_deref() else {
            continue;
        };
        if entry.db_type == DatabaseType::SQLite {
            return Err(ConnectionStoreError::InvalidPasswordReference(
                reference.to_string(),
            ));
        }
        let stable_prefix = format!("connection:{}", entry.id);
        let owned = reference == stable_prefix
            || reference
                .strip_prefix(&format!("{stable_prefix}:"))
                .is_some_and(|suffix| Uuid::parse_str(suffix).is_ok());
        if !owned {
            return Err(ConnectionStoreError::InvalidPasswordReference(
                reference.to_string(),
            ));
        }
        if !references.insert(reference) {
            return Err(ConnectionStoreError::DuplicatePasswordReference(
                reference.to_string(),
            ));
        }
        if entry.password.is_some() {
            return Err(ConnectionStoreError::InvalidPasswordReference(
                reference.to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::app_config_file::CONFIG_FILE_NAME;
    use super::*;
    use crate::app::ports::outbound::SecretStoreError;
    use crate::domain::connection::SslMode;
    use crate::domain::connection::{
        ConnectionConfig, DatabaseType, MySqlSslMode, PostgresConnectionConfig,
    };
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[derive(Default)]
    pub(super) struct TestSecretStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl SecretStore for TestSecretStore {
        fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError> {
            self.values
                .lock()
                .expect("test secret store lock poisoned")
                .insert(reference.to_string(), secret.to_string());
            Ok(())
        }

        fn get(&self, reference: &str) -> Result<String, SecretStoreError> {
            self.values
                .lock()
                .expect("test secret store lock poisoned")
                .get(reference)
                .cloned()
                .ok_or(SecretStoreError::OperationFailed)
        }

        fn delete(&self, reference: &str) -> Result<(), SecretStoreError> {
            self.values
                .lock()
                .expect("test secret store lock poisoned")
                .remove(reference);
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSecretStore {
        state: Mutex<SecretStoreState>,
    }

    #[derive(Default)]
    struct SecretStoreState {
        values: HashMap<String, String>,
        set_error: Option<SecretStoreError>,
        get_error: Option<SecretStoreError>,
        delete_error: Option<SecretStoreError>,
    }

    impl RecordingSecretStore {
        fn set_error(&self, error: SecretStoreError) {
            self.state.lock().unwrap().set_error = Some(error);
        }

        fn get_error(&self, error: SecretStoreError) {
            self.state.lock().unwrap().get_error = Some(error);
        }

        fn delete_error(&self, error: SecretStoreError) {
            self.state.lock().unwrap().delete_error = Some(error);
        }
    }

    impl SecretStore for RecordingSecretStore {
        fn set(&self, reference: &str, secret: &str) -> Result<(), SecretStoreError> {
            let mut state = self.state.lock().unwrap();
            if let Some(error) = &state.set_error {
                return Err(error.clone());
            }
            state
                .values
                .insert(reference.to_string(), secret.to_string());
            Ok(())
        }

        fn get(&self, reference: &str) -> Result<String, SecretStoreError> {
            let state = self.state.lock().unwrap();
            if let Some(error) = &state.get_error {
                return Err(error.clone());
            }
            state
                .values
                .get(reference)
                .cloned()
                .ok_or(SecretStoreError::OperationFailed)
        }

        fn delete(&self, reference: &str) -> Result<(), SecretStoreError> {
            let mut state = self.state.lock().unwrap();
            if let Some(error) = &state.delete_error {
                return Err(error.clone());
            }
            state.values.remove(reference);
            Ok(())
        }
    }

    fn store_with_secret_store(
        temp_dir: &TempDir,
        secret_store: Arc<RecordingSecretStore>,
    ) -> TomlConnectionStore {
        TomlConnectionStore::with_config_dir_and_secret_store(
            temp_dir.path().to_path_buf(),
            secret_store,
        )
    }

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
                    expected: 3
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

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profiles = store.load_all().unwrap();

            assert_eq!(profiles.len(), 1);
            assert_eq!(profiles[0].database_type(), DatabaseType::PostgreSQL);
            assert_eq!(profiles[0].postgres_config().unwrap().database, "testdb");
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

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profiles = store.load_all().unwrap();

            assert_eq!(profiles.len(), 1);
            let config = profiles[0].postgres_config().unwrap();
            assert_eq!(config.username, "");
            assert_eq!(config.host, "");
        }
    }

    mod save {
        use super::*;

        #[test]
        fn stores_postgres_password_in_secret_store() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = make_test_profile("PostgreSQL");

            store.save(&profile).unwrap();

            let content = fs::read_to_string(store.storage_path()).unwrap();
            assert!(!content.contains("testpass"));
            assert!(content.contains("password_ref = \"connection:"));

            let reloaded = store.load_all().unwrap();
            assert_eq!(reloaded[0].postgres_config().unwrap().password, "testpass");
        }

        #[test]
        fn stores_mysql_password_in_secret_store() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = ConnectionProfile::new_mysql(
                "MySQL",
                "localhost",
                3306,
                Some("testdb".to_string()),
                "testuser",
                "testpass",
                MySqlSslMode::Preferred,
            )
            .unwrap();

            store.save(&profile).unwrap();

            let content = fs::read_to_string(store.storage_path()).unwrap();
            assert!(!content.contains("testpass"));
            assert!(content.contains("password_ref = \"connection:"));
            assert_eq!(
                store.load_all().unwrap()[0]
                    .mysql_config()
                    .unwrap()
                    .password,
                "testpass"
            );
        }

        #[test]
        fn sqlite_connection_does_not_create_a_secret_reference() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = ConnectionProfile::new_sqlite("SQLite", "/tmp/test.db").unwrap();

            store.save(&profile).unwrap();

            let content = fs::read_to_string(store.storage_path()).unwrap();
            assert!(!content.contains("password_ref"));
            assert_eq!(
                store.load_all().unwrap()[0].database_type(),
                DatabaseType::SQLite
            );
        }

        #[test]
        fn empty_password_is_stored_without_plaintext_or_secret_reference() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = ConnectionProfile::new_postgres(
                "Passwordless",
                "localhost",
                5432,
                "testdb",
                "testuser",
                "",
                SslMode::Prefer,
            )
            .unwrap();

            store.save(&profile).unwrap();

            let content = fs::read_to_string(store.storage_path()).unwrap();
            assert!(!content.contains("password ="));
            assert!(!content.contains("password_ref"));
            assert_eq!(
                store.load_all().unwrap()[0]
                    .postgres_config()
                    .unwrap()
                    .password,
                ""
            );
        }

        #[test]
        fn editing_legacy_plaintext_connection_migrates_only_that_connection() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                r#"version = 3

[[connections]]
id = "legacy-one"
name = "Legacy One"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "one-password"
ssl_mode = "prefer"

[[connections]]
id = "legacy-two"
name = "Legacy Two"
host = "localhost"
port = 5432
database = "testdb"
username = "testuser"
password = "two-password"
ssl_mode = "prefer"
"#,
            )
            .unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = store
                .find_by_id(&ConnectionId::from_string("legacy-one"))
                .unwrap()
                .unwrap();

            store.save(&profile).unwrap();

            let content = fs::read_to_string(config_path).unwrap();
            assert!(content.contains("password_ref = \"connection:legacy-one\""));
            assert!(!content.contains("one-password"));
            assert!(content.contains("password = \"two-password\""));
        }

        #[test]
        fn clearing_password_removes_existing_secret_reference() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let mut profile = make_test_profile("Password");
            store.save(&profile).unwrap();
            let reference = format!("connection:{}", profile.id);

            profile.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "localhost",
                5432,
                "testdb",
                "testuser",
                "",
                SslMode::Prefer,
            ));
            store.save(&profile).unwrap();

            let content = fs::read_to_string(store.storage_path()).unwrap();
            assert!(!content.contains("password_ref"));
            assert!(matches!(
                secret_store.get(&reference),
                Err(SecretStoreError::OperationFailed)
            ));
        }

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

            profile.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "newhost",
                5432,
                "testdb",
                "testuser",
                "testpass",
                SslMode::Prefer,
            ));
            let result = store.save(&profile);

            assert!(result.is_ok());
        }

        #[test]
        fn password_update_rotates_reference_before_removing_old_secret() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let mut profile = make_test_profile("Production");
            store.save(&profile).unwrap();
            let initial: ConnectionConfigFile =
                toml::from_str(&fs::read_to_string(store.storage_path()).unwrap()).unwrap();
            let old_reference = initial.connections[0].password_ref.clone().unwrap();

            profile.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "localhost",
                5432,
                "testdb",
                "testuser",
                "newpass",
                SslMode::Prefer,
            ));
            store.save(&profile).unwrap();

            let updated: ConnectionConfigFile =
                toml::from_str(&fs::read_to_string(store.storage_path()).unwrap()).unwrap();
            let new_reference = updated.connections[0].password_ref.clone().unwrap();
            assert_ne!(new_reference, old_reference);
            assert_eq!(
                store.load_all().unwrap()[0]
                    .postgres_config()
                    .unwrap()
                    .password,
                "newpass"
            );
            assert!(
                !secret_store
                    .state
                    .lock()
                    .unwrap()
                    .values
                    .contains_key(&old_reference)
            );
        }

        #[test]
        fn secret_cleanup_failure_restores_previous_connection() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let mut profile = make_test_profile("Production");
            store.save(&profile).unwrap();
            let before = fs::read_to_string(store.storage_path()).unwrap();

            profile.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "localhost",
                5432,
                "testdb",
                "testuser",
                "newpass",
                SslMode::Prefer,
            ));
            secret_store.delete_error(SecretStoreError::OperationFailed);

            let result = store.save(&profile);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::SecretStore(
                    SecretStoreError::OperationFailed
                ))
            ));
            assert_eq!(fs::read_to_string(store.storage_path()).unwrap(), before);
            assert_eq!(
                store.load_all().unwrap()[0]
                    .postgres_config()
                    .unwrap()
                    .password,
                "testpass"
            );
        }

        #[test]
        fn secret_store_failure_leaves_existing_config_unchanged() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = make_test_profile("Production");
            store.save(&profile).unwrap();
            let before = fs::read_to_string(store.storage_path()).unwrap();

            secret_store.set_error(SecretStoreError::OperationFailed);
            let result = store.save(&profile);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::SecretStore(
                    SecretStoreError::OperationFailed
                ))
            ));
            assert_eq!(fs::read_to_string(store.storage_path()).unwrap(), before);
        }

        #[test]
        fn rejects_password_reference_owned_by_another_connection() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                r#"version = 3

[[connections]]
id = "first"
name = "First"
password_ref = "connection:second"
"#,
            )
            .unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::InvalidPasswordReference(reference))
                    if reference == "connection:second"
            ));
        }

        #[test]
        fn rejects_duplicate_password_references() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                r#"version = 3

[[connections]]
id = "same"
name = "First"
password_ref = "connection:same"

[[connections]]
id = "same"
name = "Second"
password_ref = "connection:same"
"#,
            )
            .unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());

            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::DuplicatePasswordReference(reference))
                    if reference == "connection:same"
            ));
        }

        #[cfg(unix)]
        #[test]
        fn config_write_failure_restores_previous_secret() {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let mut profile = make_test_profile("Production");
            store.save(&profile).unwrap();
            let before = fs::read_to_string(store.storage_path()).unwrap();

            profile.config = ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "newhost",
                5432,
                "testdb",
                "testuser",
                "newpass",
                SslMode::Prefer,
            ));
            fs::set_permissions(temp_dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
            let result = store.save(&profile);
            fs::set_permissions(temp_dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

            assert!(matches!(result, Err(ConnectionStoreError::Io(_))));
            assert_eq!(fs::read_to_string(store.storage_path()).unwrap(), before);
            assert_eq!(
                store.load_all().unwrap()[0]
                    .postgres_config()
                    .unwrap()
                    .password,
                "testpass"
            );
        }

        #[test]
        fn preserves_existing_app_settings() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join(CONFIG_FILE_NAME);
            fs::write(
                &config_path,
                "version = 2\ntheme = \"light\"\nkeymap_preset = \"ide\"\ner_browser = \"Firefox\"\nclipboard_backend = \"osc52\"\nconnections = []\n",
            )
            .unwrap();
            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
            let profile = make_test_profile("Test");

            store.save(&profile).unwrap();

            let content = fs::read_to_string(config_path).unwrap();
            assert!(content.contains("theme = \"light\""));
            assert!(content.contains("keymap_preset = \"ide\""));
            assert!(content.contains("er_browser = \"Firefox\""));
            assert!(content.contains("clipboard_backend = \"osc52\""));
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
        fn removes_password_from_secret_store() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();

            store.delete(&profile.id).unwrap();

            assert!(store.load_all().unwrap().is_empty());
            assert!(matches!(
                secret_store.get(&format!("connection:{}", profile.id)),
                Err(SecretStoreError::OperationFailed)
            ));
        }

        #[test]
        fn secret_store_delete_failure_preserves_config() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();
            let before = fs::read_to_string(store.storage_path()).unwrap();

            secret_store.delete_error(SecretStoreError::OperationFailed);
            let result = store.delete(&profile.id);

            assert!(matches!(
                result,
                Err(ConnectionStoreError::SecretStore(
                    SecretStoreError::OperationFailed
                ))
            ));
            assert_eq!(fs::read_to_string(store.storage_path()).unwrap(), before);
        }

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
        fn secret_store_read_failure_does_not_return_partial_profiles() {
            let temp_dir = TempDir::new().unwrap();
            let secret_store = Arc::new(RecordingSecretStore::default());
            let store = store_with_secret_store(&temp_dir, Arc::clone(&secret_store));
            let profile = make_test_profile("Test");
            store.save(&profile).unwrap();
            secret_store.get_error(SecretStoreError::OperationFailed);

            let result = store.load_all();

            assert!(matches!(
                result,
                Err(ConnectionStoreError::SecretStore(
                    SecretStoreError::OperationFailed
                ))
            ));
        }

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

            let store = TomlConnectionStore::with_config_dir(temp_dir.path().to_path_buf());
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
            assert!(all.iter().any(|p| p.name.as_str() == "First"));
            assert!(all.iter().any(|p| {
                p.name.as_str() == "Second"
                    && p.postgres_config()
                        .is_some_and(|config| config.host == "updated-host")
            }));
        }
    }
}
