//! Integration tests for PostgresAdapter (Tier 2).
//!
//! All tests require a running PostgreSQL instance and are marked `#[ignore]`.
//! Start one with `docker compose up -d --wait`, then run:
//! `cargo nextest run -p sabiql --run-ignored ignored-only -E 'test(tests::adapter_postgres)'`
//!
//! DSN is read from `SABIQL_TEST_DSN` env var.
//! Default: `postgres://dev:dev@localhost:5433/testdb` (matches compose.yml)
//! The database user must be able to create and drop schemas.

use sabiql_app::ports::outbound::{AccessMode, DbOperationError, MetadataProvider, QueryExecutor};
use sabiql_infra::adapters::postgres::PostgresAdapter;

use crate::tests::harness::postgres::{
    postgres_bad_dsn, postgres_integration_dsn, with_postgres_test_db,
};

mod metadata_fetch {
    use super::*;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn fetch_metadata_returns_schemas() {
        with_postgres_test_db(|db| {
            Box::pin(async move {
                let metadata = db
                    .adapter()
                    .fetch_metadata(db.dsn())
                    .await
                    .map_err(|err| err.to_string())?;

                if !metadata.schemas.iter().any(|s| s.name == db.schema()) {
                    return Err(format!(
                        "expected fixture schema '{}' in metadata",
                        db.schema()
                    ));
                }
                if !metadata
                    .table_summaries
                    .iter()
                    .any(|table| table.schema == db.schema() && table.name == db.table())
                {
                    return Err(format!(
                        "expected fixture table '{}.{}' in metadata",
                        db.schema(),
                        db.table()
                    ));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn fetch_table_detail_returns_columns() {
        with_postgres_test_db(|db| {
            Box::pin(async move {
                let detail = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), db.schema(), db.table())
                    .await
                    .map_err(|err| err.to_string())?;

                if detail.columns.is_empty() {
                    return Err("expected at least one column for fixture table".to_string());
                }
                Ok(())
            })
        })
        .await;
    }
}

mod query_execution {
    use super::*;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn execute_preview_returns_columns() {
        with_postgres_test_db(|db| {
            Box::pin(async move {
                let result = db
                    .adapter()
                    .execute_preview(db.dsn(), db.schema(), db.table(), 10, 0)
                    .await
                    .map_err(|err| err.to_string())?;

                if result.columns.is_empty() {
                    return Err("expected columns for fixture table preview".to_string());
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn execute_adhoc_select_returns_query_result() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadWrite)
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
        let dsn = postgres_integration_dsn();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS a; SELECT 2 AS b, 3 AS c",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["b", "c"]);
        assert_eq!(result.rows(), vec![vec!["2", "3"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn data_rows_identical_to_header_are_preserved() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        let sql = "SELECT 1 AS id, 'Alice' AS name; \
                   SELECT 'id' AS id, 'name' AS name";
        let result = adapter
            .execute_adhoc(&dsn, sql, AccessMode::ReadWrite)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["id", "name"]);
        assert_eq!(result.rows(), vec![vec!["id", "name"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn empty_leading_result_set_returns_last() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        let sql = "SELECT 1 AS a WHERE false; SELECT 2 AS b";
        let result = adapter
            .execute_adhoc(&dsn, sql, AccessMode::ReadWrite)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["b"]);
        assert_eq!(result.rows(), vec![vec!["2"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn empty_trailing_result_set_returns_zero_rows() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        let sql = "SELECT 1 AS a; SELECT 2 AS b WHERE false";
        let result = adapter
            .execute_adhoc(&dsn, sql, AccessMode::ReadWrite)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["b"]);
        assert!(result.rows().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn statements_share_one_session_and_transaction() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        // Temp tables are session-local: this only works if all
        // statements run in a single psql invocation.
        let sql = "CREATE TEMP TABLE boundary_probe(v int); \
                   INSERT INTO boundary_probe VALUES (7); \
                   SELECT v FROM boundary_probe";
        let result = adapter
            .execute_adhoc(&dsn, sql, AccessMode::ReadWrite)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["v"]);
        assert_eq!(result.rows(), vec![vec!["7"]]);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn write_only_statements_report_aggregate_tag() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_integration_dsn();

        let sql = "CREATE TEMP TABLE tag_probe(v int); \
                   INSERT INTO tag_probe VALUES (1), (2)";
        let result = adapter
            .execute_adhoc(&dsn, sql, AccessMode::ReadWrite)
            .await
            .unwrap();

        assert!(result.rows().is_empty());
        assert_eq!(
            result.command_tag,
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn error_in_later_statement_rolls_back_earlier_writes() {
        with_postgres_test_db(|db| {
            Box::pin(async move {
                let failing = format!(
                    "CREATE TABLE \"{}\".boundary_rollback_probe(v int); SELECT no_such_column",
                    db.schema()
                );
                let result = db
                    .adapter()
                    .execute_adhoc(db.dsn(), &failing, AccessMode::ReadWrite)
                    .await;
                if result.is_ok() {
                    return Err("expected mid-script error".to_string());
                }

                let check = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "SELECT to_regclass('{}.boundary_rollback_probe') IS NULL AS rolled_back",
                            db.schema()
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                if check.rows() != vec![vec!["t"]] {
                    return Err(format!("expected rollback marker, got {:?}", check.rows()));
                }
                Ok(())
            })
        })
        .await;
    }
}

mod error_paths {
    use super::*;

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn bad_dsn_returns_connection_or_query_error() {
        let adapter = PostgresAdapter::new();

        let result = adapter.fetch_metadata(postgres_bad_dsn()).await;

        assert!(result.is_err(), "Expected error for bad DSN");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, tracked: #133"]
    async fn timeout_with_pg_sleep_returns_timeout_error() {
        let adapter = PostgresAdapter::with_timeout(1);
        let dsn = postgres_integration_dsn();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT pg_sleep(5)", AccessMode::ReadWrite)
            .await;

        assert!(
            matches!(result, Err(DbOperationError::Timeout(_))),
            "Expected Timeout error, got: {result:?}"
        );
    }
}
