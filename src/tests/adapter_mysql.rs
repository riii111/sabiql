//! Integration tests for the MySQL connection probe.
//!
//! These tests require an Oracle MySQL 8.4 server and the matching `mysql` CLI.
//! Set `SABIQL_MYSQL_TEST_PASSWORD` to a password containing option-file syntax
//! characters, then run:
//! `cargo nextest run -p sabiql --run-ignored ignored-only -E 'test(tests::adapter_mysql)'`

use sabiql_app::ports::outbound::{ConnectionProbe, DsnBuilder};
use sabiql_domain::connection::{ConnectionProfile, MySqlSslMode};
use sabiql_infra::adapters::mysql::MySqlAdapter;

fn mysql_integration_profile() -> ConnectionProfile {
    let password = std::env::var("SABIQL_MYSQL_TEST_PASSWORD")
        .expect("SABIQL_MYSQL_TEST_PASSWORD must be set for MySQL integration tests");
    assert!(
        password
            .chars()
            .any(|character| " #;=\\\"".contains(character)),
        "the integration password must contain an option-file syntax character"
    );

    ConnectionProfile::new_mysql(
        "mysql-integration",
        std::env::var("SABIQL_MYSQL_TEST_HOST").unwrap_or_else(|_| "localhost".to_string()),
        std::env::var("SABIQL_MYSQL_TEST_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .unwrap_or(3306),
        std::env::var("SABIQL_MYSQL_TEST_DATABASE").ok(),
        std::env::var("SABIQL_MYSQL_TEST_USER").unwrap_or_else(|_| "root".to_string()),
        password,
        MySqlSslMode::Disabled,
    )
    .expect("integration MySQL connection settings must be valid")
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 CLI/server"]
async fn probe_real_mysql_84_uses_special_password_and_tcp() {
    let adapter = MySqlAdapter::new();
    let profile = mysql_integration_profile();
    let dsn = adapter.build_dsn(&profile);

    adapter
        .probe(&dsn)
        .await
        .expect("Oracle MySQL 8.4 TCP probe should succeed");
}
