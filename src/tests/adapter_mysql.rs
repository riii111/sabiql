//! Integration tests for the Oracle MySQL 8.4 server and mysql CLI.
//!
//! Start the exact fixture and CLI wrapper with:
//! bash scripts/mysql_integration.sh test

use sabiql_app::ports::outbound::{
    AccessMode, DbOperationError, MYSQL_SQL_MODE_UNSUPPORTED_MARKER, QueryExecutor,
};
use sabiql_domain::{CommandTag, QueryValue, RefreshScope};

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
async fn executes_multiple_statements_and_returns_only_the_last_result() {
    with_mysql_test_db(|db| Box::pin(async move {
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!(
                    "UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'multi statement' WHERE id = 1; SELECT empty_text FROM {MYSQL_FIXTURE_TABLE} WHERE id = 1"
                ),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        if result.columns != ["empty_text"]
            || result.values() != [[QueryValue::Text("multi statement".to_string())]]
            || result.command_tag != Some(CommandTag::Update(1))
            || result.refresh_scope != RefreshScope::Data
        {
            return Err(format!("unexpected multi-statement result: {result:?}"));
        }
        db.adapter()
            .execute_adhoc(
                db.dsn(),
                &format!("UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = '' WHERE id = 1"),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("failed to restore fixture: {error:?}"))?;
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn keeps_refresh_scope_and_discards_partial_result_after_a_later_failure() {
    with_mysql_test_db(|db| Box::pin(async move {
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!(
                    "UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'changed before failure' WHERE id = 1; SELECT missing_column FROM {MYSQL_FIXTURE_TABLE}"
                ),
                AccessMode::ReadWrite,
            )
            .await;
        if !matches!(
            result,
            Err(DbOperationError::QueryFailedAfterChange {
                refresh_scope: RefreshScope::Data,
                ..
            })
        ) {
            return Err(format!("expected partial-change failure: {result:?}"));
        }
        db.adapter()
            .execute_adhoc(
                db.dsn(),
                &format!("UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = '' WHERE id = 1"),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("failed to restore fixture: {error:?}"))?;
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn preserves_explicit_transaction_order_and_scope() {
    with_mysql_test_db(|db| Box::pin(async move {
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!(
                    "BEGIN; UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'rolled back' WHERE id = 1; ROLLBACK; SELECT empty_text FROM {MYSQL_FIXTURE_TABLE} WHERE id = 1"
                ),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        if result.columns != ["empty_text"]
            || result.values() != [[QueryValue::Text(String::new())]]
            || result.command_tag != Some(CommandTag::Select(1))
            || result.refresh_scope != RefreshScope::Data
        {
            return Err(format!("unexpected transaction result: {result:?}"));
        }
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn keeps_temporary_table_state_inside_one_submission() {
    with_mysql_test_db(|db| Box::pin(async move {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("system clock error: {error}"))?
            .as_nanos();
        let table = format!("sabiql_sab386_tmp_{suffix}");
        let result = db
            .adapter()
            .execute_adhoc(
                db.dsn(),
                &format!(
                    "CREATE TEMPORARY TABLE {table} (id INT); INSERT INTO {table} VALUES (1), (2); SELECT id FROM {table} ORDER BY id; DROP TEMPORARY TABLE {table}"
                ),
                AccessMode::ReadWrite,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        if result.columns != ["id"]
            || result.values() != [[QueryValue::Text("1".to_string())], [QueryValue::Text("2".to_string())]]
            || result.command_tag != Some(CommandTag::Insert(2))
            || result.refresh_scope != RefreshScope::Metadata
        {
            return Err(format!("unexpected temporary-table result: {result:?}"));
        }
        Ok(())
    }))
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn reports_metadata_scope_when_a_later_ddl_statement_fails() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("system clock error: {error}"))?
                .as_nanos();
            let table = format!("sabiql_sab386_{suffix}");
            let result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("CREATE TABLE {table} (id INT); CREATE TABLE {table} (id INT)"),
                    AccessMode::ReadWrite,
                )
                .await;
            if !matches!(
                result,
                Err(DbOperationError::QueryFailedAfterChange {
                    refresh_scope: RefreshScope::Metadata,
                    ..
                })
            ) {
                return Err(format!("expected metadata-scope failure: {result:?}"));
            }
            db.adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("DROP TABLE IF EXISTS {table}"),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("failed to clean up DDL fixture: {error:?}"))?;
            Ok(())
        })
    })
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
