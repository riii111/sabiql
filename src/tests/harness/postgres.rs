use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use sabiql_app::ports::outbound::{AccessMode, DbOperationError};
use sabiql_domain::connection::{PostgresConnectionConfig, PostgresEndpoint, SslMode};
use sabiql_infra::adapters::PostgresAdapter;
use url::Url;

const DEFAULT_TEST_DSN: &str = "postgres://dev:dev@localhost:5433/testdb";
const TEST_DSN_ENV: &str = "SABIQL_TEST_DSN";
const FIXTURE_TABLE: &str = "fixture_people";

pub struct PostgresTestDb {
    adapter: PostgresAdapter,
    dsn: PostgresEndpoint,
    schema: String,
}

type PostgresFixtureTest<'db> = Pin<Box<dyn Future<Output = Result<(), String>> + 'db>>;

impl PostgresTestDb {
    pub async fn setup() -> Result<Self, DbOperationError> {
        let db = Self {
            adapter: PostgresAdapter::new(),
            dsn: postgres_integration_dsn(),
            schema: unique_schema_name(),
        };
        db.create_fixture_schema().await?;
        Ok(db)
    }

    pub fn adapter(&self) -> &PostgresAdapter {
        &self.adapter
    }

    pub fn dsn(&self) -> &PostgresEndpoint {
        &self.dsn
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn table(&self) -> &str {
        FIXTURE_TABLE
    }

    pub async fn cleanup(&self) -> Result<(), DbOperationError> {
        self.adapter
            .execute_adhoc(
                &self.dsn,
                &format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", self.schema),
                AccessMode::ReadWrite,
            )
            .await
            .map(|_| ())
    }

    async fn create_fixture_schema(&self) -> Result<(), DbOperationError> {
        let schema = &self.schema;
        let sql = format!(
            r#"
            CREATE SCHEMA "{schema}";
            CREATE TABLE "{schema}"."{FIXTURE_TABLE}" (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            INSERT INTO "{schema}"."{FIXTURE_TABLE}" (id, name)
            VALUES (1, 'Ada'), (2, 'Grace');
            "#
        );
        self.adapter
            .execute_adhoc(&self.dsn, &sql, AccessMode::ReadWrite)
            .await
            .map(|_| ())
    }
}

pub async fn with_postgres_test_db<F>(test: F)
where
    F: for<'db> FnOnce(&'db PostgresTestDb) -> PostgresFixtureTest<'db>,
{
    let db = PostgresTestDb::setup().await.unwrap();
    let test_result = test(&db).await;
    let cleanup_result = db.cleanup().await;

    match (test_result, cleanup_result) {
        (Ok(()), Ok(())) => {}
        (Err(test_err), Ok(())) => panic!("{test_err}"),
        (Ok(()), Err(cleanup_err)) => panic!("cleanup failed: {cleanup_err}"),
        (Err(test_err), Err(cleanup_err)) => {
            panic!("test failed: {test_err}; cleanup failed: {cleanup_err}")
        }
    }
}

pub fn postgres_integration_dsn() -> PostgresEndpoint {
    let dsn = std::env::var(TEST_DSN_ENV).unwrap_or_else(|_| DEFAULT_TEST_DSN.to_string());
    postgres_endpoint_from_url(&dsn)
}

fn postgres_endpoint_from_url(dsn: &str) -> PostgresEndpoint {
    let url = Url::parse(dsn).expect("SABIQL_TEST_DSN must be a PostgreSQL URL");
    let decode = |value: &str| urlencoding::decode(value).unwrap().into_owned();
    let host = url.host_str().unwrap_or_default();
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let ssl_mode = url.query_pairs().find(|(key, _)| key == "sslmode").map_or(
        SslMode::Prefer,
        |(_, value)| {
            SslMode::all_variants()
                .iter()
                .copied()
                .find(|mode| mode.as_str() == value)
                .expect("SABIQL_TEST_DSN has an unsupported sslmode")
        },
    );
    PostgresEndpoint::Settings(PostgresConnectionConfig::new(
        decode(host),
        url.port().unwrap_or(5432),
        decode(url.path().strip_prefix('/').unwrap_or_default()),
        decode(url.username()),
        decode(url.password().unwrap_or_default()),
        ssl_mode,
    ))
}

#[test]
fn uri_path_decodes_percent_encoded_dbname() {
    let endpoint = postgres_endpoint_from_url("postgres://localhost/my%2Fdb");
    match endpoint {
        PostgresEndpoint::Settings(config) => assert_eq!(config.database, "my/db"),
        PostgresEndpoint::Service(_) => panic!("expected PostgreSQL settings"),
    }
}

pub fn postgres_bad_dsn() -> PostgresEndpoint {
    PostgresEndpoint::Settings(PostgresConnectionConfig::new(
        "127.0.0.1",
        59999,
        "nonexistent",
        "nobody",
        "wrong",
        SslMode::Prefer,
    ))
}

fn unique_schema_name() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "sabiql_it_{}_{}_{}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}
