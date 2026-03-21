//! Integration tests for PostgresAdapter (Tier 2).
//!
//! All tests require a running PostgreSQL instance and are marked `#[ignore]`.
//! Run with: `cargo test -- --ignored` or `mise run test:ignored`
//!
//! DSN is read from `SABIQL_TEST_DSN` env var.
//! Default: `postgres://postgres:postgres@localhost:5432/sabiql_test`

use sabiql::app::ports::{DbOperationError, MetadataProvider, QueryExecutor};
use sabiql::infra::adapters::postgres::PostgresAdapter;

fn test_dsn() -> String {
    std::env::var("SABIQL_TEST_DSN")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/sabiql_test".to_string())
}

mod metadata_fetch {
    use super::*;

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn fetch_metadata_returns_schemas() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert!(
            !metadata.schemas.is_empty(),
            "Expected at least one schema (public)"
        );
        assert!(
            metadata.schemas.iter().any(|s| s.name == "public"),
            "Expected public schema"
        );
    }

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn fetch_table_detail_returns_columns() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert!(
            !metadata.table_summaries.is_empty(),
            "Test DB must have at least one table; create one before running integration tests"
        );

        let first_table = &metadata.table_summaries[0];
        let detail = adapter
            .fetch_table_detail(&dsn, &first_table.schema, &first_table.name)
            .await
            .unwrap();

        assert!(
            !detail.columns.is_empty(),
            "Expected at least one column for table '{}'",
            first_table.name
        );
    }
}

mod query_execution {
    use super::*;

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn execute_preview_returns_columns() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert!(
            !metadata.table_summaries.is_empty(),
            "Test DB must have at least one table; create one before running integration tests"
        );

        let table = &metadata.table_summaries[0];
        let result = adapter
            .execute_preview(&dsn, &table.schema, &table.name, 10, 0, false)
            .await
            .unwrap();

        assert!(
            !result.columns.is_empty(),
            "Expected columns for table '{}'",
            table.name
        );
    }

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn execute_adhoc_select_returns_query_result() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT 1 AS value", false)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0], vec!["1"]);
    }
}

mod error_paths {
    use super::*;

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn bad_dsn_returns_connection_or_query_error() {
        let adapter = PostgresAdapter::new();
        let bad_dsn = "postgres://nobody:wrong@127.0.0.1:59999/nonexistent";

        let result = adapter.fetch_metadata(bad_dsn).await;

        assert!(result.is_err(), "Expected error for bad DSN");
    }

    #[tokio::test]
    #[ignore] // tracked: #133 — requires PostgreSQL
    async fn timeout_with_pg_sleep_returns_timeout_error() {
        let adapter = PostgresAdapter::with_timeout(1);
        let dsn = test_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT pg_sleep(5)", false)
            .await;

        assert!(
            matches!(result, Err(DbOperationError::Timeout)),
            "Expected Timeout error, got: {result:?}"
        );
    }
}
