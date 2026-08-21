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
        MySqlSslMode::Disabled,
    )
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
        MySqlSslMode::VerifyCa,
    )
    .with_tls_paths(
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_CA").expect("TLS CA path")),
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_CERT").expect("TLS client certificate path")),
        Some(std::env::var("SABIQL_MYSQL_TEST_SSL_KEY").expect("TLS client key path")),
    )
}
