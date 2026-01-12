use serde::{Deserialize, Serialize};

use crate::domain::connection::{
    ConnectionId, ConnectionName, ConnectionNameError, ConnectionProfile, SslMode,
};

pub const CURRENT_VERSION: u32 = 2;

/// Version-only struct for detecting config file version before full parse.
#[derive(Debug, Deserialize)]
pub struct ConfigVersionCheck {
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigFile {
    pub version: u32,
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

impl ConnectionConfigFile {
    pub fn from_profiles(profiles: &[ConnectionProfile]) -> Self {
        Self {
            version: CURRENT_VERSION,
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

    pub fn to_profiles(&self) -> Result<Vec<ConnectionProfile>, ConnectionNameError> {
        self.connections
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
