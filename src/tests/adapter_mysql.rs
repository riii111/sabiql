//! Integration tests for the Oracle MySQL 8.4 server and mysql CLI.
//!
//! Start the exact fixture and CLI wrapper with:
//! bash scripts/mysql_integration.sh test

use sabiql_app::ports::outbound::{
    AccessMode, DbOperationError, MYSQL_SQL_MODE_UNSUPPORTED_MARKER, QueryExecutor,
};
use sabiql_domain::QueryValue;

use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, with_mysql_test_db};

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn connects_to_oracle_mysql_84_fixture() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("SELECT id FROM {MYSQL_FIXTURE_TABLE}"),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("{error:?}"))?;
            if result.columns != ["id"] || result.values() != [[QueryValue::Text("1".to_string())]]
            {
                return Err(format!("unexpected MySQL connection result: {result:?}"));
            }
            Ok(())
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn preserves_xml_value_boundaries_for_real_mysql_results() {
    with_mysql_test_db(|db| Box::pin(async move {
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!(
                    "SELECT nullable_text, empty_text, unicode_text, JSON_EXTRACT(json_value, '$.array'), JSON_EXTRACT(json_value, '$.text'), blob_value FROM {MYSQL_FIXTURE_TABLE}"
                ),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        let expected = vec![
            QueryValue::Null,
            QueryValue::Text(String::new()),
            QueryValue::Text("日本語の値 🐬".to_string()),
            QueryValue::Text("[1, true]".to_string()),
            QueryValue::Text("\"空文字ではない\"".to_string()),
            QueryValue::Text("0x00FF10".to_string()),
        ];
        if result.values() != [expected] {
            return Err(format!("unexpected XML values: {:?}", result.values()));
        }
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn discards_real_cli_results_when_query_fails() {
    with_mysql_test_db(|db| Box::pin(async move {
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!("SELECT missing_column FROM {MYSQL_FIXTURE_TABLE}"),
                AccessMode::ReadWrite,
            )
            .await;
        if !matches!(result, Err(DbOperationError::QueryFailed(ref details)) if details.contains("missing_column")) {
            return Err(format!("expected a query failure without a result: {result:?}"));
        }
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn times_out_real_cli_query_and_discards_output() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let result = db
                .adapter()
                .execute_adhoc(db.dsn(), "SELECT SLEEP(32)", AccessMode::ReadWrite)
                .await;
            if !matches!(result, Err(DbOperationError::Timeout(_))) {
                return Err(format!("expected a query timeout: {result:?}"));
            }
            Ok(())
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn rejects_next_process_when_global_sql_mode_is_unsupported() {
    with_mysql_test_db(|db| Box::pin(async move {
        let original = db.global_sql_mode().await?;
        let unsupported = if original.is_empty() {
            "ANSI_QUOTES".to_string()
        } else {
            format!("{original},ANSI_QUOTES")
        };
        let test_result = async {
            db.set_global_sql_mode(&unsupported).await?;
            let result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    "SELECT SLEEP(32)",
                    AccessMode::ReadWrite,
                )
                .await;
            if !matches!(result, Err(DbOperationError::UnsupportedOperation(ref details)) if details.contains(MYSQL_SQL_MODE_UNSUPPORTED_MARKER)) {
                return Err(format!("expected unsupported sql_mode rejection: {result:?}"));
            }
            Ok::<(), String>(())
        }
        .await;
        let restore_result = db.set_global_sql_mode(&original).await;
        if let Err(error) = restore_result {
            return Err(format!("failed to restore MySQL global sql_mode: {error}"));
        }
        if db.global_sql_mode().await? != original {
            return Err("MySQL global sql_mode was not restored".to_string());
        }
        test_result
    }))
    .await;
}
