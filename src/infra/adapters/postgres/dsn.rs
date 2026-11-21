use crate::domain::connection::{PostgresConnectionConfig, PostgresEndpoint};

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every PostgreSQL endpoint must specify its connection string."
)]
pub(in crate::adapters) fn postgres_conninfo(endpoint: &PostgresEndpoint) -> String {
    match endpoint {
        PostgresEndpoint::Settings(config) => settings_conninfo(config),
        PostgresEndpoint::Service(service_name) => format!("service={service_name}"),
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every PostgreSQL endpoint must specify its reported database name."
)]
pub(super) fn database_name(endpoint: &PostgresEndpoint) -> String {
    match endpoint {
        PostgresEndpoint::Settings(config) => {
            let name = config.database.trim();
            if name.is_empty() {
                "unknown".to_string()
            } else {
                name.to_string()
            }
        }
        PostgresEndpoint::Service(name) => name.clone(),
    }
}

fn settings_conninfo(config: &PostgresConnectionConfig) -> String {
    let mut parts = Vec::new();
    push_conninfo_part(&mut parts, "host", config.host.trim());
    push_conninfo_part(&mut parts, "port", &config.port.to_string());
    push_conninfo_part(&mut parts, "dbname", config.database.trim());
    push_conninfo_part(&mut parts, "user", config.username.trim());
    push_conninfo_part(&mut parts, "password", config.password.as_str());
    push_conninfo_part(&mut parts, "sslmode", &config.ssl_mode.to_string());
    parts.join(" ")
}

fn push_conninfo_part(parts: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        parts.push(format!("{key}={}", quote_conninfo_value(value)));
    }
}

fn quote_conninfo_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::{ConnectionProfile, SslMode};

    mod dsn_builder {
        use super::*;

        #[test]
        fn quotes_special_chars() {
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "/var/run/postgresql",
                5432,
                "my db",
                "user'org",
                "p\\ss word",
                SslMode::Prefer,
            )
            .unwrap();
            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.postgres_config().unwrap().clone(),
            ));
            assert_eq!(
                dsn,
                "host='/var/run/postgresql' port='5432' dbname='my db' user='user\\'org' password='p\\\\ss word' sslmode='prefer'"
            );
        }

        #[test]
        fn omits_empty_userinfo_and_host() {
            let profile =
                ConnectionProfile::new_postgres("Test", "", 5432, "mydb", "", "", SslMode::Prefer)
                    .unwrap();

            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.postgres_config().unwrap().clone(),
            ));

            assert_eq!(dsn, "port='5432' dbname='mydb' sslmode='prefer'");
        }

        #[test]
        fn includes_password_without_user() {
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "",
                5432,
                "mydb",
                "",
                "secret",
                SslMode::Prefer,
            )
            .unwrap();

            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.postgres_config().unwrap().clone(),
            ));

            assert_eq!(
                dsn,
                "port='5432' dbname='mydb' password='secret' sslmode='prefer'"
            );
        }
    }

    mod reported_database_name {
        use super::*;
        use rstest::rstest;

        fn database_name_for_settings(database: &str) -> String {
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "localhost",
                5432,
                database,
                "user",
                "secret",
                SslMode::Prefer,
            )
            .unwrap();
            database_name(&PostgresEndpoint::Settings(
                profile.postgres_config().unwrap().clone(),
            ))
        }

        fn database_name_for_service(service_name: &str) -> String {
            let endpoint = PostgresEndpoint::Service(service_name.to_string());
            database_name(&endpoint)
        }

        #[rstest]
        #[case::inner_spaces("my db", "my db")]
        #[case::surrounding_spaces("  padded  ", "padded")]
        #[case::single_quote("it's", "it's")]
        #[case::backslash("C:\\db", "C:\\db")]
        #[case::slash("my/db", "my/db")]
        fn saved_settings_report_the_trimmed_database_name(
            #[case] database: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(database_name_for_settings(database), expected);
        }

        #[test]
        fn saved_settings_keep_percent_sequences_in_the_database_name() {
            assert_eq!(database_name_for_settings("a%20b"), "a%20b");
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace("   ")]
        fn saved_settings_without_a_database_report_unknown(#[case] database: &str) {
            assert_eq!(database_name_for_settings(database), "unknown");
        }

        #[test]
        fn service_connections_report_the_service_name() {
            assert_eq!(database_name_for_service("my service"), "my service");
        }
    }
}
