use serde::{Deserialize, Serialize};

use crate::app::model::shared::theme_id::ThemeId;
use crate::app::ports::outbound::AppSettings;
use crate::domain::connection::{
    ConnectionId, ConnectionName, ConnectionNameError, ConnectionProfile, SslMode,
};

pub const CURRENT_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
pub struct ConfigVersionCheck {
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    pub connections: Vec<ConnectionConfigEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigEntry {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
}

impl From<&[ConnectionProfile]> for ConnectionConfigFile {
    fn from(profiles: &[ConnectionProfile]) -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: None,
            connections: profiles
                .iter()
                .map(|p| ConnectionConfigEntry {
                    id: p.id.as_str().to_string(),
                    name: p.name.as_str().to_string(),
                    host: p.host.clone(),
                    port: p.port,
                    database: p.database.clone(),
                    username: p.username.clone(),
                    password: p.password.clone(),
                    ssl_mode: p.ssl_mode,
                })
                .collect(),
        }
    }
}

impl ConnectionConfigFile {
    pub fn app_settings(&self) -> AppSettings {
        if self.version != CURRENT_VERSION {
            return AppSettings::default();
        }

        AppSettings {
            theme_id: self
                .theme
                .as_deref()
                .and_then(ThemeId::from_config_value)
                .unwrap_or_default(),
        }
    }

    pub fn set_app_settings(&mut self, settings: AppSettings) {
        self.theme = Some(settings.theme_id.config_value().to_string());
    }
}

impl TryFrom<&ConnectionConfigFile> for Vec<ConnectionProfile> {
    type Error = ConnectionNameError;

    fn try_from(config: &ConnectionConfigFile) -> Result<Self, Self::Error> {
        config
            .connections
            .iter()
            .map(|entry| {
                Ok(ConnectionProfile {
                    id: ConnectionId::from_string(&entry.id),
                    name: ConnectionName::new(&entry.name)?,
                    host: entry.host.clone(),
                    port: entry.port,
                    database: entry.database.clone(),
                    username: entry.username.clone(),
                    password: entry.password.clone(),
                    ssl_mode: entry.ssl_mode,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_theme_maps_to_app_settings() {
        let config = ConnectionConfigFile {
            version: CURRENT_VERSION,
            theme: Some("light".to_string()),
            connections: vec![],
        };

        assert_eq!(config.app_settings().theme_id, ThemeId::Light);
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let config = ConnectionConfigFile {
            version: CURRENT_VERSION,
            theme: Some("terminal".to_string()),
            connections: vec![],
        };

        assert_eq!(config.app_settings().theme_id, ThemeId::Default);
    }

    #[test]
    fn missing_theme_falls_back_to_default() {
        let config = ConnectionConfigFile {
            version: CURRENT_VERSION,
            theme: None,
            connections: vec![],
        };

        assert_eq!(config.app_settings().theme_id, ThemeId::Default);
    }
}
