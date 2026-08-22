use sabiql_app::ports::outbound::{DbOperationError, DsnBuilder};
use sabiql_domain::connection::{ConnectionProfile, MySqlConnectionConfig, MySqlSslMode};
#[cfg(unix)]
use sabiql_infra::adapters::mysql::run_mysql_cli_script_for_test;
use sabiql_infra::adapters::mysql::{MySqlAdapter, run_mysql_cli_query_for_test};

pub const MYSQL_FIXTURE_TABLE: &str = "mysql_cli_fixture";

type MySqlFixtureTest<'db> = std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + 'db>>;

pub struct MySqlTestDb {
    adapter: MySqlAdapter,
    dsn: String,
}

impl MySqlTestDb {
    pub fn setup() -> Result<Self, DbOperationError> {
        let config = mysql_integration_config();
        let profile = ConnectionProfile::new_mysql(
            "mysql-integration",
            config.host.clone(),
            config.port,
            config.database.clone(),
            config.username.clone(),
            config.password.clone(),
            config.ssl_mode,
        )
        .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?;
        let adapter = MySqlAdapter::new();
        let dsn = adapter.build_dsn(&profile);
        Ok(Self { adapter, dsn })
    }

    pub fn adapter(&self) -> &MySqlAdapter {
        &self.adapter
    }

    pub fn dsn(&self) -> &str {
        &self.dsn
    }

    pub async fn global_sql_mode(&self) -> Result<String, String> {
        self.run_cli("SELECT @@GLOBAL.sql_mode")
            .await
            .map(|output| output.trim().to_string())
    }

    pub async fn set_global_sql_mode(&self, sql_mode: &str) -> Result<(), String> {
        let escaped = sql_mode.replace('\'', "''");
        self.run_cli(&format!("SET GLOBAL sql_mode = '{escaped}'"))
            .await
            .map(|_| ())
    }

    async fn run_cli(&self, query: &str) -> Result<String, String> {
        run_mysql_cli_query_for_test(&self.dsn, query)
            .await
            .map_err(|error| error.to_string())
    }

    #[cfg(unix)]
    pub async fn run_pty_script(&self, script: &str) -> Result<Vec<u8>, String> {
        run_mysql_cli_script_for_test(self.dsn(), script)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn run_cli_script(&self, script: &str) -> Result<String, String> {
        self.run_cli(script).await
    }
}

pub async fn with_mysql_test_db<F>(test: F)
where
    F: for<'db> FnOnce(&'db MySqlTestDb) -> MySqlFixtureTest<'db>,
{
    let db = MySqlTestDb::setup().unwrap();
    let result = test(&db).await;
    if let Err(error) = result {
        panic!("{error}");
    }
}

fn mysql_config(ssl_mode: MySqlSslMode) -> MySqlConnectionConfig {
    MySqlConnectionConfig::new(
        std::env::var("SABIQL_MYSQL_TEST_HOST")
            .unwrap_or_else(|_| "host.docker.internal".to_string()),
        std::env::var("SABIQL_MYSQL_TEST_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .unwrap_or(3306),
        Some(
            std::env::var("SABIQL_MYSQL_TEST_DATABASE")
                .unwrap_or_else(|_| "sabiql_test".to_string()),
        ),
        std::env::var("SABIQL_MYSQL_TEST_USER")
            .unwrap_or_else(|_| "sabiql_test_runner".to_string()),
        std::env::var("SABIQL_MYSQL_TEST_PASSWORD")
            .unwrap_or_else(|_| "p a#ss;=\"word".to_string()),
        ssl_mode,
    )
}

pub fn mysql_integration_config() -> MySqlConnectionConfig {
    mysql_config(MySqlSslMode::Disabled)
        .with_server_public_key_path(std::env::var("SABIQL_MYSQL_TEST_SERVER_PUBLIC_KEY").ok())
}

pub fn mysql_cache_miss_config() -> MySqlConnectionConfig {
    let mut config = mysql_integration_config();
    config.username = std::env::var("SABIQL_MYSQL_TEST_CACHE_MISS_USER")
        .unwrap_or_else(|_| "sabiql_cache_miss_runner".to_string());
    config.password = std::env::var("SABIQL_MYSQL_TEST_CACHE_MISS_PASSWORD")
        .unwrap_or_else(|_| "sabiql-cache-miss".to_string());
    config
}

pub fn mysql_tls_config() -> MySqlConnectionConfig {
    mysql_config(MySqlSslMode::VerifyCa).with_tls_paths(
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_CA").expect("TLS CA path")),
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_CERT").expect("TLS client certificate path")),
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_KEY").expect("TLS client key path")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvironmentGuard {
        original_values: Vec<(&'static str, Option<String>)>,
    }

    impl EnvironmentGuard {
        fn new(names: &[&'static str]) -> Self {
            Self {
                original_values: names
                    .iter()
                    .map(|&name| (name, std::env::var(name).ok()))
                    .collect(),
            }
        }

        fn set(&self, name: &'static str, value: &str) {
            // SAFETY: callers hold ENV_LOCK for the duration of the test.
            unsafe { std::env::set_var(name, value) };
        }

        fn remove(&self, name: &'static str) {
            // SAFETY: callers hold ENV_LOCK for the duration of the test.
            unsafe { std::env::remove_var(name) };
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            for &(name, ref value) in &self.original_values {
                // SAFETY: the guard is dropped while its test still owns ENV_LOCK.
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(name, value),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    fn assert_common_fields_equal(left: &MySqlConnectionConfig, right: &MySqlConnectionConfig) {
        assert_eq!(left.host, right.host);
        assert_eq!(left.port, right.port);
        assert_eq!(left.database, right.database);
        assert_eq!(left.username, right.username);
        assert_eq!(left.password, right.password);
    }

    #[test]
    fn mysql_config_uses_the_same_defaults_for_both_ssl_modes() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let environment = EnvironmentGuard::new(&[
            "SABIQL_MYSQL_TEST_HOST",
            "SABIQL_MYSQL_TEST_PORT",
            "SABIQL_MYSQL_TEST_DATABASE",
            "SABIQL_MYSQL_TEST_USER",
            "SABIQL_MYSQL_TEST_PASSWORD",
        ]);
        for name in [
            "SABIQL_MYSQL_TEST_HOST",
            "SABIQL_MYSQL_TEST_PORT",
            "SABIQL_MYSQL_TEST_DATABASE",
            "SABIQL_MYSQL_TEST_USER",
            "SABIQL_MYSQL_TEST_PASSWORD",
        ] {
            environment.remove(name);
        }

        let integration = mysql_config(MySqlSslMode::Disabled);
        let tls = mysql_config(MySqlSslMode::VerifyCa);

        assert_common_fields_equal(&integration, &tls);
        assert_eq!(integration.host, "host.docker.internal");
        assert_eq!(integration.port, 3306);
        assert_eq!(integration.database.as_deref(), Some("sabiql_test"));
        assert_eq!(integration.username, "sabiql_test_runner");
        assert_eq!(integration.password, "p a#ss;=\"word");
        assert_eq!(integration.ssl_mode, MySqlSslMode::Disabled);
        assert_eq!(tls.ssl_mode, MySqlSslMode::VerifyCa);
    }

    #[test]
    fn wrappers_share_env_values_and_keep_tls_specific_settings() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let environment = EnvironmentGuard::new(&[
            "SABIQL_MYSQL_TEST_HOST",
            "SABIQL_MYSQL_TEST_PORT",
            "SABIQL_MYSQL_TEST_DATABASE",
            "SABIQL_MYSQL_TEST_USER",
            "SABIQL_MYSQL_TEST_PASSWORD",
            "SABIQL_MYSQL_TEST_SERVER_PUBLIC_KEY",
            "SABIQL_MYSQL_TEST_SSL_CA",
            "SABIQL_MYSQL_TEST_SSL_CERT",
            "SABIQL_MYSQL_TEST_SSL_KEY",
        ]);
        for (name, value) in [
            ("SABIQL_MYSQL_TEST_HOST", "mysql.example.test"),
            ("SABIQL_MYSQL_TEST_PORT", "13306"),
            ("SABIQL_MYSQL_TEST_DATABASE", "fixture_database"),
            ("SABIQL_MYSQL_TEST_USER", "fixture_user"),
            ("SABIQL_MYSQL_TEST_PASSWORD", "fixture_password"),
            (
                "SABIQL_MYSQL_TEST_SERVER_PUBLIC_KEY",
                "/tmp/server-public-key.pem",
            ),
            ("SABIQL_MYSQL_TEST_SSL_CA", "/tmp/ca.pem"),
            ("SABIQL_MYSQL_TEST_SSL_CERT", "/tmp/client-cert.pem"),
            ("SABIQL_MYSQL_TEST_SSL_KEY", "/tmp/client-key.pem"),
        ] {
            environment.set(name, value);
        }

        let integration = mysql_integration_config();
        let tls = mysql_tls_config();

        assert_common_fields_equal(&integration, &tls);
        assert_eq!(integration.ssl_mode, MySqlSslMode::Disabled);
        assert_eq!(
            integration.server_public_key_path.as_deref(),
            Some("/tmp/server-public-key.pem")
        );
        assert_eq!(tls.ssl_mode, MySqlSslMode::VerifyCa);
        assert_eq!(tls.ssl_ca.as_deref(), Some("/tmp/ca.pem"));
        assert_eq!(tls.ssl_cert.as_deref(), Some("/tmp/client-cert.pem"));
        assert_eq!(tls.ssl_key.as_deref(), Some("/tmp/client-key.pem"));
    }
}
