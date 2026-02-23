use serde::{Deserialize, Serialize};

use super::id::ConnectionId;
use super::name::{ConnectionName, ConnectionNameError};
use super::ssl_mode::SslMode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: ConnectionId,
    pub name: ConnectionName,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
}

impl ConnectionProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionNameError> {
        Ok(Self {
            id: ConnectionId::new(),
            name: ConnectionName::new(name)?,
            host: host.into(),
            port,
            database: database.into(),
            username: username.into(),
            password: password.into(),
            ssl_mode,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: ConnectionId,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        ssl_mode: SslMode,
    ) -> Result<Self, ConnectionNameError> {
        Ok(Self {
            id,
            name: ConnectionName::new(name)?,
            host: host.into(),
            port,
            database: database.into(),
            username: username.into(),
            password: password.into(),
            ssl_mode,
        })
    }

    pub fn display_name(&self) -> &str {
        self.name.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_profile() -> ConnectionProfile {
        ConnectionProfile::new(
            "Test Connection",
            "localhost",
            5432,
            "testdb",
            "testuser",
            "testpass",
            SslMode::Prefer,
        )
        .unwrap()
    }

    mod new {
        use super::*;

        #[test]
        fn generates_unique_id() {
            let p1 = make_test_profile();
            let p2 = make_test_profile();
            assert_ne!(p1.id, p2.id);
        }

        #[test]
        fn empty_name_returns_error() {
            let result = ConnectionProfile::new(
                "",
                "localhost",
                5432,
                "testdb",
                "testuser",
                "testpass",
                SslMode::Prefer,
            );
            assert!(result.is_err());
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn returns_connection_name() {
            let profile = make_test_profile();
            assert_eq!(profile.display_name(), "Test Connection");
        }
    }
}
