//! Integration tests for PostgresAdapter (Tier 2).
//!
//! All tests require a running PostgreSQL instance and are marked `#[ignore]`.
//! Start one with `docker compose up -d --wait`, then run:
//! `cargo nextest run -p sabiql --run-ignored ignored-only -E 'test(tests::adapter_postgres)'`
//!
//! DSN is read from `SABIQL_TEST_DSN` env var.
//! Default: `postgres://dev:dev@localhost:5433/testdb` (matches compose.yml)

use sabiql_app::ports::outbound::{DbOperationError, MetadataProvider, QueryExecutor};
use sabiql_infra::adapters::postgres::PostgresAdapter;

fn test_dsn() -> String {
    std::env::var("SABIQL_TEST_DSN")
        .unwrap_or_else(|_| "postgres://dev:dev@localhost:5433/testdb".to_string())
}

mod metadata_fetch {
    use super::*;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
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
    #[ignore = "requires PostgreSQL, tracked: #133"]
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
    #[ignore = "requires PostgreSQL, tracked: #133"]
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
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn execute_adhoc_select_returns_query_result() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT 1 AS value", false)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows().len(), 1);
        assert_eq!(result.rows()[0], vec!["1"]);
    }
}

mod multi_statement_boundaries {
    use super::*;
    use sabiql_domain::CommandTag;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn returns_last_result_set() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT 1 AS a; SELECT 2 AS b, 3 AS c", false)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["b", "c"]);
        assert_eq!(result.rows(), vec![vec!["2", "3"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn data_rows_identical_to_header_are_preserved() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let sql = "SELECT 1 AS id, 'Alice' AS name; \
                   SELECT 'id' AS id, 'name' AS name";
        let result = adapter.execute_adhoc(&dsn, sql, false).await.unwrap();

        assert_eq!(result.columns, vec!["id", "name"]);
        assert_eq!(result.rows(), vec![vec!["id", "name"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn empty_leading_result_set_returns_last() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let sql = "SELECT 1 AS a WHERE false; SELECT 2 AS b";
        let result = adapter.execute_adhoc(&dsn, sql, false).await.unwrap();

        assert_eq!(result.columns, vec!["b"]);
        assert_eq!(result.rows(), vec![vec!["2"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn empty_trailing_result_set_returns_zero_rows() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let sql = "SELECT 1 AS a; SELECT 2 AS b WHERE false";
        let result = adapter.execute_adhoc(&dsn, sql, false).await.unwrap();

        assert_eq!(result.columns, vec!["b"]);
        assert!(result.rows().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn statements_share_one_session_and_transaction() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        // Temp tables are session-local: this only works if all
        // statements run in a single psql invocation.
        let sql = "CREATE TEMP TABLE boundary_probe(v int); \
                   INSERT INTO boundary_probe VALUES (7); \
                   SELECT v FROM boundary_probe";
        let result = adapter.execute_adhoc(&dsn, sql, false).await.unwrap();

        assert_eq!(result.columns, vec!["v"]);
        assert_eq!(result.rows(), vec![vec!["7"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn write_only_statements_report_aggregate_tag() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();

        let sql = "CREATE TEMP TABLE tag_probe(v int); \
                   INSERT INTO tag_probe VALUES (1), (2)";
        let result = adapter.execute_adhoc(&dsn, sql, false).await.unwrap();

        assert!(result.rows().is_empty());
        assert_eq!(
            result.command_tag,
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn error_in_later_statement_rolls_back_earlier_writes() {
        let adapter = PostgresAdapter::new();
        let dsn = test_dsn();
        adapter
            .execute_adhoc(&dsn, "DROP TABLE IF EXISTS boundary_rollback_probe", false)
            .await
            .unwrap();

        let failing = "CREATE TABLE boundary_rollback_probe(v int); SELECT no_such_column";
        let result = adapter.execute_adhoc(&dsn, failing, false).await;
        assert!(result.is_err(), "expected mid-script error, got {result:?}");

        let check = adapter
            .execute_adhoc(
                &dsn,
                "SELECT to_regclass('boundary_rollback_probe') IS NULL AS rolled_back",
                false,
            )
            .await
            .unwrap();
        assert_eq!(check.rows(), vec![vec!["t"]]);
    }
}

mod error_paths {
    use super::*;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn bad_dsn_returns_connection_or_query_error() {
        let adapter = PostgresAdapter::new();
        let bad_dsn = "postgres://nobody:wrong@127.0.0.1:59999/nonexistent";

        let result = adapter.fetch_metadata(bad_dsn).await;

        assert!(result.is_err(), "Expected error for bad DSN");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn timeout_with_pg_sleep_returns_timeout_error() {
        let adapter = PostgresAdapter::with_timeout(1);
        let dsn = test_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT pg_sleep(5)", false)
            .await;

        assert!(
            matches!(result, Err(DbOperationError::Timeout(_))),
            "Expected Timeout error, got: {result:?}"
        );
    }
}
