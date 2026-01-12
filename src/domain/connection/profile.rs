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

    /// Special characters in credentials are URL-encoded
    pub fn to_dsn(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            urlencoding::encode(&self.username),
            urlencoding::encode(&self.password),
            &self.host,
            self.port,
            urlencoding::encode(&self.database),
            self.ssl_mode
        )
    }

    /// For logging - password replaced with ****
    pub fn to_masked_dsn(&self) -> String {
        format!(
            "postgres://{}:****@{}:{}/{}?sslmode={}",
            self.username, self.host, self.port, self.database, self.ssl_mode
        )
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

    mod to_dsn {
        use super::*;

        #[test]
        fn includes_all_connection_fields() {
            let profile = make_test_profile();
            let dsn = profile.to_dsn();
            assert!(dsn.starts_with("postgres://"));
            assert!(dsn.contains("testuser"));
            assert!(dsn.contains("testpass"));
            assert!(dsn.contains("localhost"));
            assert!(dsn.contains("5432"));
            assert!(dsn.contains("testdb"));
            assert!(dsn.contains("sslmode=prefer"));
        }

        #[test]
        fn encodes_special_chars_in_credentials() {
            let profile = ConnectionProfile::new(
                "Test",
                "localhost",
                5432,
                "my/db",
                "user@org",
                "p@ss:word",
                SslMode::Prefer,
            )
            .unwrap();
            let dsn = profile.to_dsn();
            assert!(dsn.contains("user%40org"));
            assert!(dsn.contains("p%40ss%3Aword"));
            assert!(dsn.contains("my%2Fdb"));
        }
    }

    mod to_masked_dsn {
        use super::*;

        #[test]
        fn hides_password() {
            let profile = make_test_profile();
            let masked = profile.to_masked_dsn();
            assert!(masked.contains("****"));
            assert!(!masked.contains("testpass"));
        }
    }
}
