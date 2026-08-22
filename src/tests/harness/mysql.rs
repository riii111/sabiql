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

pub fn mysql_integration_config() -> MySqlConnectionConfig {
    mysql_config(MySqlSslMode::Disabled)
        .with_server_public_key_path(std::env::var("SABIQL_MYSQL_TEST_SERVER_PUBLIC_KEY").ok())
}

fn mysql_config(ssl_mode: MySqlSslMode) -> MySqlConnectionConfig {
    mysql_config_with(ssl_mode, |name| std::env::var(name).ok())
}

fn mysql_config_with(
    ssl_mode: MySqlSslMode,
    env: impl Fn(&str) -> Option<String>,
) -> MySqlConnectionConfig {
    MySqlConnectionConfig::new(
        env("SABIQL_MYSQL_TEST_HOST").unwrap_or_else(|| "host.docker.internal".to_string()),
        env("SABIQL_MYSQL_TEST_PORT")
            .and_then(|port| port.parse().ok())
            .unwrap_or(3306),
        Some(env("SABIQL_MYSQL_TEST_DATABASE").unwrap_or_else(|| "sabiql_test".to_string())),
        env("SABIQL_MYSQL_TEST_USER").unwrap_or_else(|| "sabiql_test_runner".to_string()),
        env("SABIQL_MYSQL_TEST_PASSWORD").unwrap_or_else(|| "p a#ss;=\"word".to_string()),
        ssl_mode,
    )
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
    use super::mysql_config_with;
    use sabiql_domain::connection::{MySqlConnectionConfig, MySqlSslMode};

    const COMMON_ENV_VALUES: [(&str, &str); 5] = [
        ("SABIQL_MYSQL_TEST_HOST", "mysql.example"),
        ("SABIQL_MYSQL_TEST_PORT", "13306"),
        ("SABIQL_MYSQL_TEST_DATABASE", "fixture_db"),
        ("SABIQL_MYSQL_TEST_USER", "fixture_user"),
        ("SABIQL_MYSQL_TEST_PASSWORD", "fixture password"),
    ];

    fn env_value(values: &[(&str, &str)], name: &str) -> Option<String> {
        values
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_string())
    }

    fn assert_common_fields_equal(
        integration: &MySqlConnectionConfig,
        tls: &MySqlConnectionConfig,
    ) {
        assert_eq!(integration.host, tls.host);
        assert_eq!(integration.port, tls.port);
        assert_eq!(integration.database, tls.database);
        assert_eq!(integration.username, tls.username);
        assert_eq!(integration.password, tls.password);
    }

    #[test]
    fn config_wrappers_share_common_env_values_and_keep_tls_specific_settings() {
        let integration = mysql_config_with(MySqlSslMode::Disabled, |name| {
            env_value(&COMMON_ENV_VALUES, name)
        })
        .with_server_public_key_path(Some("/fixtures/server-key.pem".to_string()));
        let tls = mysql_config_with(MySqlSslMode::VerifyCa, |name| {
            env_value(&COMMON_ENV_VALUES, name)
        })
        .with_tls_paths(
            Some("/fixtures/ca.pem".to_string()),
            Some("/fixtures/client-cert.pem".to_string()),
            Some("/fixtures/client-key.pem".to_string()),
        );

        assert_common_fields_equal(&integration, &tls);
        assert_eq!(integration.host, "mysql.example");
        assert_eq!(integration.port, 13306);
        assert_eq!(integration.database.as_deref(), Some("fixture_db"));
        assert_eq!(integration.username, "fixture_user");
        assert_eq!(integration.password, "fixture password");
        assert_eq!(integration.ssl_mode, MySqlSslMode::Disabled);
        assert_eq!(
            integration.server_public_key_path.as_deref(),
            Some("/fixtures/server-key.pem")
        );
        assert_eq!(tls.ssl_mode, MySqlSslMode::VerifyCa);
        assert_eq!(tls.ssl_ca.as_deref(), Some("/fixtures/ca.pem"));
        assert_eq!(tls.ssl_cert.as_deref(), Some("/fixtures/client-cert.pem"));
        assert_eq!(tls.ssl_key.as_deref(), Some("/fixtures/client-key.pem"));
        assert_eq!(tls.server_public_key_path, None);
    }

    #[test]
    fn config_wrappers_share_common_fallback_values_when_env_is_missing() {
        let integration = mysql_config_with(MySqlSslMode::Disabled, |_| None);
        let tls = mysql_config_with(MySqlSslMode::VerifyCa, |_| None).with_tls_paths(
            Some("/fixtures/ca.pem".to_string()),
            Some("/fixtures/client-cert.pem".to_string()),
            Some("/fixtures/client-key.pem".to_string()),
        );

        assert_common_fields_equal(&integration, &tls);
        assert_eq!(integration.host, "host.docker.internal");
        assert_eq!(integration.port, 3306);
        assert_eq!(integration.database.as_deref(), Some("sabiql_test"));
        assert_eq!(integration.username, "sabiql_test_runner");
        assert_eq!(integration.password, "p a#ss;=\"word");
        assert_eq!(integration.server_public_key_path, None);
        assert_eq!(tls.ssl_ca.as_deref(), Some("/fixtures/ca.pem"));
        assert_eq!(tls.ssl_cert.as_deref(), Some("/fixtures/client-cert.pem"));
        assert_eq!(tls.ssl_key.as_deref(), Some("/fixtures/client-key.pem"));
    }
}
