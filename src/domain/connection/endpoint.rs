use std::fmt;

use super::config::{
    ConnectionConfig, MySqlConnectionConfig, PostgresConnectionConfig, SqliteConnectionConfig,
};
use super::database_type::DatabaseType;
use super::service_entry::ServiceEntry;

const MASKED_PASSWORD: &str = "****";

#[derive(Clone, PartialEq, Eq)]
pub enum ConnectionEndpoint {
    PostgreSQL(PostgresEndpoint),
    SQLite(SqliteConnectionConfig),
    MySQL(MySqlConnectionConfig),
}

#[derive(Clone, PartialEq, Eq)]
pub enum PostgresEndpoint {
    Settings(PostgresConnectionConfig),
    Service(String),
}

impl ConnectionEndpoint {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    pub fn engine(&self) -> DatabaseType {
        match self {
            Self::PostgreSQL(_) => DatabaseType::PostgreSQL,
            Self::SQLite(_) => DatabaseType::SQLite,
            Self::MySQL(_) => DatabaseType::MySQL,
        }
    }

    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    pub fn selected_database(&self) -> Option<&str> {
        match self {
            Self::MySQL(config) => config.database.as_deref(),
            Self::PostgreSQL(_) | Self::SQLite(_) => None,
        }
    }

    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    pub fn requires_database_selection(&self) -> bool {
        match self {
            Self::MySQL(config) => config.database.is_none(),
            Self::PostgreSQL(_) | Self::SQLite(_) => false,
        }
    }

    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    pub fn is_postgres_service(&self) -> bool {
        match self {
            Self::PostgreSQL(PostgresEndpoint::Service(_)) => true,
            Self::PostgreSQL(PostgresEndpoint::Settings(_)) | Self::SQLite(_) | Self::MySQL(_) => {
                false
            }
        }
    }
}

impl From<&ConnectionConfig> for ConnectionEndpoint {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    fn from(config: &ConnectionConfig) -> Self {
        match config {
            ConnectionConfig::PostgreSQL(config) => {
                Self::PostgreSQL(PostgresEndpoint::Settings(config.clone()))
            }
            ConnectionConfig::SQLite(config) => Self::SQLite(config.clone()),
            ConnectionConfig::MySQL(config) => Self::MySQL(config.clone()),
        }
    }
}

impl From<&ServiceEntry> for ConnectionEndpoint {
    fn from(entry: &ServiceEntry) -> Self {
        Self::PostgreSQL(PostgresEndpoint::Service(entry.service_name.clone()))
    }
}

impl fmt::Debug for ConnectionEndpoint {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its own answer so a new engine fails to compile instead of silently inheriting another engine's behavior."
    )]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PostgreSQL(endpoint) => {
                formatter.debug_tuple("PostgreSQL").field(endpoint).finish()
            }
            Self::SQLite(config) => formatter.debug_tuple("SQLite").field(config).finish(),
            Self::MySQL(config) => {
                let masked = MySqlConnectionConfig {
                    password: MASKED_PASSWORD.to_string(),
                    ..config.clone()
                };
                formatter.debug_tuple("MySQL").field(&masked).finish()
            }
        }
    }
}

impl fmt::Debug for PostgresEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Settings(config) => {
                let masked = PostgresConnectionConfig {
                    password: MASKED_PASSWORD.to_string(),
                    ..config.clone()
                };
                formatter.debug_tuple("Settings").field(&masked).finish()
            }
            Self::Service(service_name) => formatter
                .debug_tuple("Service")
                .field(service_name)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::connection::{MySqlSslMode, SslMode};

    fn postgres_settings() -> PostgresConnectionConfig {
        PostgresConnectionConfig::new(
            "pg.example",
            5432,
            "app",
            "app_user",
            "pg-secret",
            SslMode::Prefer,
        )
    }

    fn sqlite_settings() -> SqliteConnectionConfig {
        SqliteConnectionConfig::new("/tmp/app.db").unwrap()
    }

    fn mysql_settings(database: Option<&str>) -> MySqlConnectionConfig {
        MySqlConnectionConfig::new(
            "mysql.example",
            3306,
            database.map(str::to_string),
            "app_user",
            "mysql-secret",
            MySqlSslMode::Required,
        )
    }

    fn service_entry() -> ServiceEntry {
        ServiceEntry {
            service_name: "my service".to_string(),
        }
    }

    mod conversion {
        use super::*;

        #[rstest]
        #[case::postgres(
            ConnectionConfig::PostgreSQL(postgres_settings()),
            ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(postgres_settings()))
        )]
        #[case::sqlite(
            ConnectionConfig::SQLite(sqlite_settings()),
            ConnectionEndpoint::SQLite(sqlite_settings())
        )]
        #[case::mysql(
            ConnectionConfig::MySQL(mysql_settings(Some("app"))),
            ConnectionEndpoint::MySQL(mysql_settings(Some("app")))
        )]
        fn saved_settings_keep_their_engine_and_values(
            #[case] config: ConnectionConfig,
            #[case] expected: ConnectionEndpoint,
        ) {
            let endpoint = ConnectionEndpoint::from(&config);

            assert_eq!(endpoint, expected);
            assert_eq!(endpoint.engine(), config.database_type());
        }

        #[test]
        fn service_entry_becomes_a_postgres_service() {
            let endpoint = ConnectionEndpoint::from(&service_entry());

            assert_eq!(
                endpoint,
                ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Service("my service".to_string()))
            );
            assert_eq!(endpoint.engine(), DatabaseType::PostgreSQL);
        }
    }

    mod database_selection {
        use super::*;

        #[test]
        fn mysql_reports_the_configured_database() {
            let endpoint = ConnectionEndpoint::MySQL(mysql_settings(Some("app")));

            assert_eq!(endpoint.selected_database(), Some("app"));
            assert!(!endpoint.requires_database_selection());
        }

        #[test]
        fn mysql_without_a_database_requires_selection() {
            let endpoint = ConnectionEndpoint::MySQL(mysql_settings(None));

            assert_eq!(endpoint.selected_database(), None);
            assert!(endpoint.requires_database_selection());
        }

        #[rstest]
        #[case::postgres_settings(ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(
            postgres_settings()
        )))]
        #[case::postgres_service(ConnectionEndpoint::from(&service_entry()))]
        #[case::sqlite(ConnectionEndpoint::SQLite(sqlite_settings()))]
        fn other_engines_neither_report_nor_require_a_database(
            #[case] endpoint: ConnectionEndpoint,
        ) {
            assert_eq!(endpoint.selected_database(), None);
            assert!(!endpoint.requires_database_selection());
        }
    }

    mod postgres_service {
        use super::*;

        #[test]
        fn service_entry_is_a_postgres_service() {
            assert!(ConnectionEndpoint::from(&service_entry()).is_postgres_service());
        }

        #[rstest]
        #[case::postgres_settings(ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(
            postgres_settings()
        )))]
        #[case::sqlite(ConnectionEndpoint::SQLite(sqlite_settings()))]
        #[case::mysql(ConnectionEndpoint::MySQL(mysql_settings(Some("app"))))]
        fn saved_settings_are_not_a_postgres_service(#[case] endpoint: ConnectionEndpoint) {
            assert!(!endpoint.is_postgres_service());
        }
    }

    mod debug {
        use super::*;

        #[test]
        fn masks_the_postgres_password() {
            let endpoint =
                ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(postgres_settings()));

            let debug = format!("{endpoint:?}");

            assert!(!debug.contains("pg-secret"));
            assert!(debug.contains(r#"password: "****""#));
            assert!(debug.contains("pg.example"));
        }

        #[test]
        fn masks_the_mysql_password() {
            let endpoint = ConnectionEndpoint::MySQL(mysql_settings(Some("app")));

            let debug = format!("{endpoint:?}");

            assert!(!debug.contains("mysql-secret"));
            assert!(debug.contains(r#"password: "****""#));
            assert!(debug.contains("mysql.example"));
        }

        #[test]
        fn shows_the_sqlite_path_and_the_service_name() {
            let sqlite = format!("{:?}", ConnectionEndpoint::SQLite(sqlite_settings()));
            let service = format!("{:?}", ConnectionEndpoint::from(&service_entry()));

            assert!(sqlite.contains("/tmp/app.db"));
            assert!(service.contains("my service"));
        }
    }
}
