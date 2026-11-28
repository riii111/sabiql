use crate::domain::connection::PostgresEndpoint;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::{ConnectionProfile, SslMode};

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
                profile.config.as_postgres().unwrap().clone(),
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
