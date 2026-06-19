use std::sync::atomic::{AtomicU64, Ordering};

use sabiql_app::ports::outbound::{DbOperationError, QueryExecutor};
use sabiql_infra::adapters::postgres::PostgresAdapter;

const DEFAULT_TEST_DSN: &str = "postgres://dev:dev@localhost:5433/testdb";
const TEST_DSN_ENV: &str = "SABIQL_TEST_DSN";
const FIXTURE_TABLE: &str = "fixture_people";

pub struct PostgresTestDb {
    adapter: PostgresAdapter,
    dsn: String,
    schema: String,
}

impl PostgresTestDb {
    pub async fn setup() -> Result<Self, DbOperationError> {
        let db = Self {
            adapter: PostgresAdapter::new(),
            dsn: postgres_test_dsn(),
            schema: unique_schema_name(),
        };
        db.create_fixture_schema().await?;
        Ok(db)
    }

    pub fn adapter(&self) -> &PostgresAdapter {
        &self.adapter
    }

    pub fn dsn(&self) -> &str {
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
                false,
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
            .execute_adhoc(&self.dsn, &sql, false)
            .await
            .map(|_| ())
    }
}

pub fn postgres_test_dsn() -> String {
    std::env::var(TEST_DSN_ENV).unwrap_or_else(|_| DEFAULT_TEST_DSN.to_string())
}

pub fn postgres_bad_dsn() -> &'static str {
    "postgres://nobody:wrong@127.0.0.1:59999/nonexistent"
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
