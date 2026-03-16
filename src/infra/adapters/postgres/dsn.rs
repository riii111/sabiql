use crate::app::ports::DsnBuilder;
use crate::domain::connection::ConnectionProfile;

use super::PostgresAdapter;

impl PostgresAdapter {
    pub fn extract_database_name(dsn: &str) -> String {
        if let Some(name) = dsn.strip_prefix("service=") {
            return name.to_string();
        }
        if let Some(db) = dsn
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty() && !s.contains('='))
        {
            return db.to_string();
        }
        const DBNAME_KEY: &str = "dbname=";
        if let Some(start) = dsn.find(DBNAME_KEY) {
            let rest = &dsn[start + DBNAME_KEY.len()..];
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            return rest[..end].to_string();
        }
        "unknown".to_string()
    }
}

impl DsnBuilder for PostgresAdapter {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            urlencoding::encode(&profile.username),
            urlencoding::encode(&profile.password),
            &profile.host,
            profile.port,
            urlencoding::encode(&profile.database),
            profile.ssl_mode
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::SslMode;

    mod dsn_builder {
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

        #[test]
        fn includes_all_connection_fields() {
            let adapter = PostgresAdapter::new();
            let profile = make_test_profile();
            let dsn = adapter.build_dsn(&profile);
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
            let adapter = PostgresAdapter::new();
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
            let dsn = adapter.build_dsn(&profile);
            assert!(dsn.contains("user%40org"));
            assert!(dsn.contains("p%40ss%3Aword"));
            assert!(dsn.contains("my%2Fdb"));
        }
    }

    mod extract_database_name {
        use super::*;

        #[test]
        fn uri_format() {
            assert_eq!(
                PostgresAdapter::extract_database_name("postgres://user:pass@host:5432/mydb"),
                "mydb"
            );
        }

        #[test]
        fn simple_uri() {
            assert_eq!(
                PostgresAdapter::extract_database_name("postgres://localhost/testdb"),
                "testdb"
            );
        }

        #[test]
        fn key_value_format() {
            assert_eq!(
                PostgresAdapter::extract_database_name("host=localhost dbname=mydb user=postgres"),
                "mydb"
            );
        }

        #[test]
        fn key_value_at_end() {
            assert_eq!(
                PostgresAdapter::extract_database_name(
                    "host=localhost user=postgres dbname=testdb"
                ),
                "testdb"
            );
        }

        #[test]
        fn empty_path() {
            assert_eq!(
                PostgresAdapter::extract_database_name("postgres://localhost/"),
                "unknown"
            );
        }

        #[test]
        fn key_value_only() {
            assert_eq!(
                PostgresAdapter::extract_database_name("host=localhost user=postgres"),
                "unknown"
            );
        }

        #[test]
        fn service_dsn_returns_service_name() {
            assert_eq!(
                PostgresAdapter::extract_database_name("service=mydb"),
                "mydb"
            );
        }
    }
}
