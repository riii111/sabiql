use std::process::Stdio;

use sabiql_app::ports::outbound::{ConnectionProbe, DbOperationError, DsnBuilder};
use sabiql_domain::connection::{ConnectionProfile, MySqlConnectionConfig, MySqlSslMode};
use sabiql_infra::adapters::mysql::MySqlAdapter;
use tempfile::NamedTempFile;
use tokio::process::Command;

pub const MYSQL_FIXTURE_TABLE: &str = "mysql_cli_fixture";

type MySqlFixtureTest<'db> = std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + 'db>>;

pub struct MySqlTestDb {
    adapter: MySqlAdapter,
    dsn: String,
    config: MySqlConnectionConfig,
}

impl MySqlTestDb {
    pub async fn setup() -> Result<Self, DbOperationError> {
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
        adapter.probe(&dsn).await?;
        Ok(Self {
            adapter,
            dsn,
            config,
        })
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
        let option_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        std::fs::write(option_file.path(), serialize_option_file(&self.config))
            .map_err(|error| error.to_string())?;
        let output = Command::new("mysql")
            .args([
                format!("--defaults-file={}", option_file.path().display()),
                "--no-login-paths".to_string(),
                "--protocol=TCP".to_string(),
                "--connect-timeout=10".to_string(),
                "--batch".to_string(),
                "--raw".to_string(),
                "--skip-column-names".to_string(),
                "--binary-mode".to_string(),
                "--skip-reconnect".to_string(),
                format!("--execute={query}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        String::from_utf8(output.stdout).map_err(|error| error.to_string())
    }
}

pub async fn with_mysql_test_db<F>(test: F)
where
    F: for<'db> FnOnce(&'db MySqlTestDb) -> MySqlFixtureTest<'db>,
{
    let db = MySqlTestDb::setup().await.unwrap();
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
        std::env::var("SABIQL_MYSQL_TEST_USER").unwrap_or_else(|_| "sabiql".to_string()),
        std::env::var("SABIQL_MYSQL_TEST_PASSWORD")
            .unwrap_or_else(|_| "p a#ss;=\"word".to_string()),
        MySqlSslMode::Disabled,
    )
}

fn serialize_option_file(config: &MySqlConnectionConfig) -> String {
    let mut contents = String::from("[client]\n");
    push_option(&mut contents, "host", &config.host);
    push_option(&mut contents, "port", &config.port.to_string());
    push_option(&mut contents, "user", &config.username);
    push_option(&mut contents, "password", &config.password);
    if let Some(database) = config.database.as_deref() {
        push_option(&mut contents, "database", database);
    }
    contents
}

fn push_option(contents: &mut String, key: &str, value: &str) {
    contents.push_str(key);
    contents.push_str(" = \"");
    for character in value.chars() {
        match character {
            '\\' => contents.push_str("\\\\"),
            '"' => contents.push_str("\\\""),
            _ => contents.push(character),
        }
    }
    contents.push_str("\"\n");
}
