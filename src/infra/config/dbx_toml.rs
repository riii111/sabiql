use std::collections::HashMap;
use std::fs;
use std::path::Path;

use color_eyre::eyre::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DbxConfig {
    #[serde(default)]
    pub default: Option<ProfileConfig>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileConfig {
    pub dsn: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

impl DbxConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: DbxConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn get_profile(&self, name: &str) -> Option<&ProfileConfig> {
        if name == "default" {
            self.default.as_ref()
        } else {
            self.profiles.get(name)
        }
    }

    pub fn resolve_dsn(&self, profile_name: &str) -> Option<String> {
        let profile = self.get_profile(profile_name)?;

        if let Some(dsn) = &profile.dsn {
            return Some(dsn.clone());
        }

        let host = profile.host.as_deref().unwrap_or("localhost");
        let port = profile.port.unwrap_or(5432);
        let user = profile.user.as_deref()?;
        let database = profile.database.as_deref()?;

        let dsn = match &profile.password {
            Some(password) => format!("postgres://{}:{}@{}:{}/{}", user, password, host, port, database),
            None => format!("postgres://{}@{}:{}/{}", user, host, port, database),
        };

        Some(dsn)
    }
}
