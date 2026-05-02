use serde::{Deserialize, Serialize};

use crate::app::model::shared::theme_id::ThemeId;
use crate::app::ports::outbound::AppSettings;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsConfigFile {
    pub version: u32,
    pub theme: String,
}

impl From<AppSettings> for SettingsConfigFile {
    fn from(settings: AppSettings) -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: settings.theme_id.config_value().to_string(),
        }
    }
}

impl From<SettingsConfigFile> for AppSettings {
    fn from(config: SettingsConfigFile) -> Self {
        if config.version != CURRENT_VERSION {
            return Self::default();
        }
        Self {
            theme_id: ThemeId::from_config_value(&config.theme).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_theme_maps_to_theme_id() {
        let settings = AppSettings::from(SettingsConfigFile {
            version: CURRENT_VERSION,
            theme: "light".to_string(),
        });

        assert_eq!(settings.theme_id, ThemeId::Light);
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let settings = AppSettings::from(SettingsConfigFile {
            version: CURRENT_VERSION,
            theme: "terminal".to_string(),
        });

        assert_eq!(settings.theme_id, ThemeId::Default);
    }

    #[test]
    fn version_mismatch_falls_back_to_default() {
        let settings = AppSettings::from(SettingsConfigFile {
            version: CURRENT_VERSION + 1,
            theme: "light".to_string(),
        });

        assert_eq!(settings.theme_id, ThemeId::Default);
    }
}
