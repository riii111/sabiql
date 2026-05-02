use std::sync::Arc;

use crate::model::shared::theme_id::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSettings {
    pub theme_id: ThemeId,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_id: ThemeId::Default,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsStoreError {
    #[error("I/O error: {0}")]
    Io(#[source] Arc<std::io::Error>),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[source] Arc<toml::ser::Error>),
}

impl From<std::io::Error> for SettingsStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

impl From<toml::ser::Error> for SettingsStoreError {
    fn from(error: toml::ser::Error) -> Self {
        Self::TomlSerialize(Arc::new(error))
    }
}

pub trait SettingsStore: Send + Sync {
    fn load(&self) -> Result<AppSettings, SettingsStoreError>;
    fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError>;
}
