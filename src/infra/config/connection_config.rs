use serde::{Deserialize, Serialize};

use crate::domain::connection::{ConnectionId, ConnectionProfile, SslMode};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigFile {
    pub version: u32,
    pub connection: ConnectionConfigEntry,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionConfigEntry {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
}

impl ConnectionConfigFile {
    pub fn from_profile(profile: &ConnectionProfile) -> Self {
        Self {
            version: CURRENT_VERSION,
            connection: ConnectionConfigEntry {
                id: profile.id.as_str().to_string(),
                host: profile.host.clone(),
                port: profile.port,
                database: profile.database.clone(),
                username: profile.username.clone(),
                password: profile.password.clone(),
                ssl_mode: profile.ssl_mode,
            },
        }
    }

    pub fn to_profile(&self) -> ConnectionProfile {
        ConnectionProfile {
            id: ConnectionId::from_string(&self.connection.id),
            host: self.connection.host.clone(),
            port: self.connection.port,
            database: self.connection.database.clone(),
            username: self.connection.username.clone(),
            password: self.connection.password.clone(),
            ssl_mode: self.connection.ssl_mode,
        }
    }
}
