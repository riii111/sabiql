//! Integration tests for the Oracle MySQL 8.4 server and mysql CLI.
//!
//! Start the exact fixture and CLI wrapper with:
//! bash scripts/mysql_integration.sh test

use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql_app::ports::outbound::{
    AccessMode, ConnectionProbe, DbOperationError, DdlGenerator, DsnBuilder,
    MYSQL_SQL_MODE_UNSUPPORTED_MARKER, MetadataProvider, QueryExecutor, SqlDialect,
};
use sabiql_domain::{
    CommandTag, DatabaseType, FkAction, IndexType, QueryValue, RefreshScope, TableKind,
    TriggerEvent, TriggerTiming,
};

use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, mysql_tls_config, with_mysql_test_db};
use sabiql_domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionProfile, MySqlConnectionConfig, MySqlSslMode,
};
use sabiql_infra::adapters::mysql::MySqlAdapter;

fn mysql_tls_profile(name: &str, config: MySqlConnectionConfig) -> ConnectionProfile {
    ConnectionProfile::with_id_and_config(
        ConnectionId::new(),
        name,
        ConnectionConfig::MySQL(config),
    )
    .unwrap()
}

const MYSQL_COMPOSITE_TABLE: &str = "mysql_preview_composite";
const MYSQL_NO_PK_TABLE: &str = "mysql_preview_no_pk";
const MYSQL_EMPTY_TABLE: &str = "mysql_preview_empty";
const MYSQL_VIEW: &str = "mysql_preview_view";
const MYSQL_FK_PARENT: &str = "mysql_metadata_parent";
const MYSQL_FK_CHILD: &str = "mysql_metadata_child";

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
#[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
async fn connects_to_oracle_mysql_84_fixture_with_ca_and_client_certificate() {
    let config = mysql_tls_config();
    let profile = mysql_tls_profile("mysql-tls-integration", config);
    let adapter = MySqlAdapter::new();
    let dsn = adapter.build_dsn(&profile);

    adapter.probe(&dsn).await.unwrap();
    let result = adapter
        .execute_adhoc(
            &dsn,
            &format!("SELECT id FROM {MYSQL_FIXTURE_TABLE}"),
            AccessMode::ReadWrite,
        )
        .await
        .unwrap();
    assert_eq!(result.values(), [[QueryValue::Text("1".to_string())]]);
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
async fn rejects_oracle_mysql_84_fixture_with_wrong_ca() {
    let mut config = mysql_tls_config();
    config.ssl_ca = config.ssl_cert.clone();
    let profile = mysql_tls_profile("mysql-tls-wrong-ca", config);
    let adapter = MySqlAdapter::new();
    let dsn = adapter.build_dsn(&profile);
    let error = adapter.probe(&dsn).await.unwrap_err();
    assert_eq!(
        ConnectionErrorInfo::from_db_operation_error_with_dsn(&error, &dsn).kind,
        ConnectionErrorKind::MySqlCaVerificationFailed
    );
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
async fn rejects_oracle_mysql_84_fixture_with_wrong_hostname() {
    let mut config = mysql_tls_config();
    config.host = "host.docker.internal".to_string();
    config.ssl_mode = MySqlSslMode::VerifyIdentity;
    let profile = mysql_tls_profile("mysql-tls-wrong-host", config);
    let adapter = MySqlAdapter::new();
    let dsn = adapter.build_dsn(&profile);
    let error = adapter.probe(&dsn).await.unwrap_err();
    let error_info = ConnectionErrorInfo::from_db_operation_error_with_dsn(&error, &dsn);
    assert_eq!(
        error_info.kind,
        ConnectionErrorKind::MySqlHostnameVerificationFailed,
        "masked connection error details: {}",
        error_info.masked_details()
    );
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
async fn loads_mysql_tables_views_and_column_attributes() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let metadata = db
                .adapter()
                .fetch_metadata(db.dsn())
                .await
                .map_err(|error| format!("{error:?}"))?;
            if metadata
                .schemas
                .iter()
                .map(|schema| schema.name.as_str())
                .collect::<Vec<_>>()
                != ["sabiql_test"]
            {
                return Err(format!("unexpected MySQL schemas: {:?}", metadata.schemas));
            }
            let view = metadata
                .table_summaries
                .iter()
                .find(|summary| summary.name == MYSQL_VIEW)
                .ok_or_else(|| "MySQL view was not listed".to_string())?;
            if view.kind_info.kind != TableKind::View {
                return Err(format!("unexpected MySQL view kind: {:?}", view.kind_info));
            }

            let detail = db
                .adapter()
                .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FIXTURE_TABLE)
                .await
                .map_err(|error| format!("{error:?}"))?;
            let names = detail
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>();
            if names
                != [
                    "id",
                    "nullable_text",
                    "empty_text",
                    "unicode_text",
                    "json_value",
                    "blob_value",
                    "invisible_value",
                    "generated_value",
                    "unsigned_value",
                    "precise_decimal",
                    "scientific_value",
                ]
            {
                return Err(format!("unexpected MySQL column order: {names:?}"));
            }
            let invisible = detail
                .columns
                .iter()
                .find(|column| column.name == "invisible_value")
                .ok_or_else(|| "invisible MySQL column was not returned".to_string())?;
            if !invisible.is_hidden() || !invisible.is_read_only() {
                return Err(format!(
                    "unexpected invisible column attributes: {invisible:?}"
                ));
            }
            let generated = detail
                .columns
                .iter()
                .find(|column| column.name == "generated_value")
                .ok_or_else(|| "generated MySQL column was not returned".to_string())?;
            if !generated.is_generated() || !generated.is_read_only() {
                return Err(format!(
                    "unexpected generated column attributes: {generated:?}"
                ));
            }
            if detail.primary_key != Some(vec!["id".to_string()]) {
                return Err(format!(
                    "unexpected MySQL primary key: {:?}",
                    detail.primary_key
                ));
            }
            if detail.comment.as_deref() != Some("MySQL fixture table")
                || detail.row_count_estimate.is_none()
            {
                return Err(format!(
                    "unexpected MySQL table info: comment={:?}, rows={:?}",
                    detail.comment, detail.row_count_estimate
                ));
            }

            let composite = db
                .adapter()
                .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_COMPOSITE_TABLE)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if composite.primary_key
                != Some(vec!["second_key".to_string(), "first_key".to_string()])
            {
                return Err(format!(
                    "unexpected composite primary key: {:?}",
                    composite.primary_key
                ));
            }
            let unique_index = composite
                .indexes
                .iter()
                .find(|index| index.name == "uq_mysql_preview_composite_payload")
                .ok_or_else(|| "MySQL unique index was not returned".to_string())?;
            if !unique_index.is_unique() || unique_index.columns != ["payload"] {
                return Err(format!("unexpected MySQL unique index: {unique_index:?}"));
            }
            let fulltext_index = composite
                .indexes
                .iter()
                .find(|index| index.name == "ft_mysql_preview_composite_payload")
                .ok_or_else(|| "MySQL fulltext index was not returned".to_string())?;
            if fulltext_index.index_type != IndexType::Other("fulltext".to_string())
                || fulltext_index.columns != ["payload"]
            {
                return Err(format!(
                    "unexpected MySQL fulltext index: {fulltext_index:?}"
                ));
            }

            let parent = db
                .adapter()
                .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FK_PARENT)
                .await
                .map_err(|error| format!("{error:?}"))?;
            let parent_unique_index = parent
                .indexes
                .iter()
                .find(|index| index.name == "uq_mysql_metadata_parent_code")
                .ok_or_else(|| "MySQL parent unique index was not returned".to_string())?;
            if !parent_unique_index.is_unique() || parent_unique_index.columns != ["unique_code"] {
                return Err(format!(
                    "unexpected MySQL parent unique index: {parent_unique_index:?}"
                ));
            }

            let child = db
                .adapter()
                .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FK_CHILD)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if child.foreign_keys.len() != 1 {
                return Err(format!(
                    "unexpected MySQL foreign keys: {:?}",
                    child.foreign_keys
                ));
            }
            let foreign_key = &child.foreign_keys[0];
            if foreign_key.from_columns != ["parent_first", "parent_second"]
                || foreign_key.to_schema != "sabiql_test"
                || foreign_key.to_table != MYSQL_FK_PARENT
                || foreign_key.to_columns != ["first_key", "second_key"]
                || foreign_key.on_update != FkAction::Cascade
                || foreign_key.on_delete != FkAction::SetNull
                || !foreign_key.is_reference_resolved()
            {
                return Err(format!("unexpected MySQL foreign key: {foreign_key:?}"));
            }

            let columns_and_fks = db
                .adapter()
                .fetch_table_columns_and_fks(db.dsn(), "sabiql_test", MYSQL_FK_CHILD)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if columns_and_fks.foreign_keys != child.foreign_keys
                || !columns_and_fks.indexes.is_empty()
            {
                return Err(format!(
                    "unexpected columns-and-fks metadata: {columns_and_fks:?}"
                ));
            }

            let signatures = db
                .adapter()
                .fetch_table_signatures(db.dsn())
                .await
                .map_err(|error| format!("{error:?}"))?;
            let child_signature = signatures
                .iter()
                .find(|signature| signature.name == MYSQL_FK_CHILD)
                .ok_or_else(|| "MySQL child signature was not returned".to_string())?;
            if !child_signature
                .signature
                .contains("fk_mysql_metadata_child_parent")
            {
                return Err(format!(
                    "unexpected MySQL table signature: {child_signature:?}"
                ));
            }
            Ok(())
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn previews_mysql_rows_with_visible_columns_types_and_pagination() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let result = db
                .adapter()
                .execute_preview(db.dsn(), "sabiql_test", MYSQL_FIXTURE_TABLE, 10, 0)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if result
                .columns
                .iter()
                .any(|column| column == "invisible_value")
            {
                return Err(format!(
                    "invisible column reached preview: {:?}",
                    result.columns
                ));
            }
            if result.columns
                != [
                    "id",
                    "nullable_text",
                    "empty_text",
                    "unicode_text",
                    "json_value",
                    "blob_value",
                    "generated_value",
                    "unsigned_value",
                    "precise_decimal",
                    "scientific_value",
                ]
            {
                return Err(format!("unexpected preview columns: {:?}", result.columns));
            }
            let values = result.values();
            if values.len() != 1
                || values[0][5] != QueryValue::Blob(vec![0, 255, 16])
                || values[0][7] != QueryValue::SqlLiteral("18446744073709551615".to_string())
                || values[0][8]
                    != QueryValue::SqlLiteral(
                        "12345678901234567890123456789012345.123456789012345678901234567890"
                            .to_string(),
                    )
            {
                return Err(format!("unexpected typed preview values: {values:?}"));
            }
            if !result.query.contains("ORDER BY `id`")
                || !result.query.contains("LIMIT 10 OFFSET 0")
            {
                return Err(format!("unexpected preview SQL: {}", result.query));
            }

            let page = db
                .adapter()
                .execute_preview(db.dsn(), "sabiql_test", MYSQL_NO_PK_TABLE, 1, 1)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if !page
                .query
                .contains("ORDER BY `duplicate_value`, `payload` LIMIT 1 OFFSET 1")
            {
                return Err(format!("unexpected no-PK preview SQL: {}", page.query));
            }

            let composite = db
                .adapter()
                .execute_preview(db.dsn(), "sabiql_test", MYSQL_COMPOSITE_TABLE, 1, 0)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if !composite
                .query
                .contains("ORDER BY `second_key`, `first_key` LIMIT 1 OFFSET 0")
            {
                return Err(format!(
                    "unexpected composite preview SQL: {}",
                    composite.query
                ));
            }

            let view = db
                .adapter()
                .execute_preview(db.dsn(), "sabiql_test", MYSQL_VIEW, 10, 0)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if view.columns != ["id", "unicode_text"] {
                return Err(format!(
                    "unexpected view preview columns: {:?}",
                    view.columns
                ));
            }
            Ok(())
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn previews_empty_mysql_table_with_metadata_columns() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let result = db
                .adapter()
                .execute_preview(db.dsn(), "sabiql_test", MYSQL_EMPTY_TABLE, 10, 0)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if result.columns != ["id", "payload"] || !result.values().is_empty() {
                return Err(format!("unexpected empty preview: {result:?}"));
            }
            Ok(())
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires Oracle MySQL 8.4 server and CLI"]
async fn updates_and_bulk_deletes_mysql_rows_with_visible_composite_primary_keys() {
    with_mysql_test_db(|db| {
        Box::pin(async move {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("system clock error: {error}"))?
                .as_nanos();
            let table = format!("mysql_sab392_{suffix}");
            let create = format!(
                "CREATE TABLE {table} (first_key BIGINT UNSIGNED NOT NULL, second_key INT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY (first_key, second_key))"
            );
            db.adapter()
                .execute_adhoc(db.dsn(), &create, AccessMode::ReadWrite)
                .await
                .map_err(|error| format!("failed to create write fixture: {error:?}"))?;

            let result = async {
                let insert = format!(
                    "INSERT INTO {table} (first_key, second_key, payload) VALUES (18446744073709551615, 7, 'before'), (42, 8, 'second')"
                );
                db.adapter()
                    .execute_adhoc(db.dsn(), &insert, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| format!("failed to seed write fixture: {error:?}"))?;

                let update_sql = db.adapter().build_update_sql(
                    DatabaseType::MySQL,
                    "sabiql_test",
                    &table,
                    "payload",
                    &QueryValue::text("O'Reilly\\path"),
                    &[
                        (
                            "first_key".to_string(),
                            QueryValue::SqlLiteral("18446744073709551615".to_string()),
                        ),
                        (
                            "second_key".to_string(),
                            QueryValue::SqlLiteral("7".to_string()),
                        ),
                    ],
                );
                let update = db
                    .adapter()
                    .execute_write(db.dsn(), &update_sql, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| format!("failed to update MySQL row: {error:?}"))?;
                if update.affected_rows != 1 {
                    return Err(format!("unexpected update result: {update:?}"));
                }

                let changed = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "SELECT payload FROM {table} WHERE first_key = 18446744073709551615 AND second_key = 7"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to read updated row: {error:?}"))?;
                if changed.values() != [[QueryValue::Text("O'Reilly\\path".to_string())]] {
                    return Err(format!("unexpected updated row: {changed:?}"));
                }

                let delete_sql = db.adapter().build_bulk_delete_sql(
                    DatabaseType::MySQL,
                    "sabiql_test",
                    &table,
                    &[
                        vec![
                            (
                                "first_key".to_string(),
                                QueryValue::SqlLiteral("18446744073709551615".to_string()),
                            ),
                            (
                                "second_key".to_string(),
                                QueryValue::SqlLiteral("7".to_string()),
                            ),
                        ],
                        vec![
                            (
                                "first_key".to_string(),
                                QueryValue::SqlLiteral("42".to_string()),
                            ),
                            (
                                "second_key".to_string(),
                                QueryValue::SqlLiteral("8".to_string()),
                            ),
                        ],
                    ],
                );
                let delete = db
                    .adapter()
                    .execute_write(db.dsn(), &delete_sql, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| format!("failed to delete MySQL rows: {error:?}"))?;
                if delete.affected_rows != 2 {
                    return Err(format!("unexpected delete result: {delete:?}"));
                }
                Ok(())
            }
            .await;

            let drop = format!("DROP TABLE {table}");
            let _ = db
                .adapter()
                .execute_adhoc(db.dsn(), &drop, AccessMode::ReadWrite)
                .await;
            result
        })
    })
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
