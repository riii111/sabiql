//! Integration tests for the Oracle MySQL 8.4 server and mysql CLI.
//!
//! Start the exact fixture and CLI wrapper with:
//! bash scripts/mysql_integration.sh test

mod shared {
    pub(super) const MYSQL_COMPOSITE_TABLE: &str = "mysql_preview_composite";
    pub(super) const MYSQL_EMPTY_TABLE: &str = "mysql_preview_empty";
    pub(super) const MYSQL_VIEW: &str = "mysql_preview_view";
}

mod connection {

    use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, mysql_tls_config, with_mysql_test_db};
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
    use sabiql_app::ports::outbound::{AccessMode, ConnectionProbe, DsnBuilder, QueryExecutor};
    use sabiql_domain::QueryValue;
    use sabiql_domain::connection::{
        ConnectionConfig, ConnectionId, ConnectionProfile, MySqlConnectionConfig, MySqlSslMode,
    };
    use sabiql_infra::adapters::mysql::MySqlAdapter;
    #[cfg(unix)]
    use tempfile::NamedTempFile;

    fn mysql_tls_profile(name: &str, config: MySqlConnectionConfig) -> ConnectionProfile {
        ConnectionProfile::with_id_and_config(
            ConnectionId::new(),
            name,
            ConnectionConfig::MySQL(config),
        )
        .unwrap()
    }

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
                if result.columns != ["id"]
                    || result.values() != [[QueryValue::Text("1".to_string())]]
                {
                    return Err(format!("unexpected MySQL connection result: {result:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[cfg(unix)]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn batch_mysql_cli_does_not_execute_shell_commands() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let marker = NamedTempFile::new().map_err(|error| error.to_string())?;
                let marker = marker.into_temp_path();
                std::fs::remove_file(&marker).map_err(|error| error.to_string())?;
                let output = db
                    .run_pty_script(&format!("SELECT 1;\n\\! touch '{}'\n", marker.display()))
                    .await
                    .map_err(|error| format!("failed to run MySQL CLI: {error}"))?;
                if !output
                    .windows(b"<resultset statement=\"SELECT 1\"".len())
                    .any(|window| window == b"<resultset statement=\"SELECT 1\"")
                {
                    return Err(format!(
                        "MySQL CLI did not execute SELECT 1 through the PTY: {}",
                        String::from_utf8_lossy(&output)
                    ));
                }
                if marker.exists() {
                    return Err(format!(
                        "MySQL CLI executed a shell command: {}",
                        String::from_utf8_lossy(&output)
                    ));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[cfg(unix)]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn preserves_control_characters_in_real_mysql_pty_input() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let mut script = b"SELECT HEX(_binary '".to_vec();
                script.extend_from_slice(&[0x03, 0x0d, 0x11, 0x13]);
                script.extend_from_slice(b"') AS control_bytes;\n");
                let script = String::from_utf8(script).map_err(|error| error.to_string())?;
                let output = db
                    .run_pty_script(&script)
                    .await
                    .map_err(|error| format!("failed to run MySQL CLI: {error}"))?;
                if !output
                    .windows(b">030D1113</field>".len())
                    .any(|window| window == b">030D1113</field>")
                {
                    return Err(format!(
                        "MySQL CLI changed control-byte input: {}",
                        String::from_utf8_lossy(&output)
                    ));
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
}

mod metadata_fetch {

    use super::shared::{MYSQL_COMPOSITE_TABLE, MYSQL_VIEW};
    use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, with_mysql_test_db};
    use sabiql_app::ports::outbound::{AccessMode, DdlGenerator, MetadataProvider, QueryExecutor};
    use sabiql_domain::{FkAction, IndexType, TableKind, TriggerEvent, TriggerTiming};

    const MYSQL_FK_PARENT: &str = "mysql_metadata_parent";
    const MYSQL_FK_CHILD: &str = "mysql_metadata_child";

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
                if detail.source_ddl().is_none()
                    || db
                        .adapter()
                        .generate_ddl(sabiql_domain::DatabaseType::MySQL, &detail)
                        != detail.source_ddl().unwrap()
                {
                    return Err(format!(
                        "unexpected MySQL table DDL: {:?}",
                        detail.source_ddl()
                    ));
                }
                if detail.triggers.len() != 1 {
                    return Err(format!("unexpected MySQL triggers: {:?}", detail.triggers));
                }
                let trigger = &detail.triggers[0];
                if trigger.name != "mysql_cli_fixture_audit"
                    || trigger.timing != TriggerTiming::Before
                    || trigger.events != [TriggerEvent::Update]
                    || trigger.definition != "SET NEW.empty_text = NEW.empty_text"
                    || trigger.security_context.as_deref() != Some("sabiql@%")
                {
                    return Err(format!("unexpected MySQL trigger: {trigger:?}"));
                }

                let view_detail = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_VIEW)
                    .await
                    .map_err(|error| format!("{error:?}"))?;
                if view_detail.source_ddl().is_none()
                    || !view_detail
                        .source_ddl()
                        .is_some_and(|ddl| ddl.contains("CREATE") && ddl.contains(MYSQL_VIEW))
                    || db
                        .adapter()
                        .generate_ddl(sabiql_domain::DatabaseType::MySQL, &view_detail)
                        != view_detail.source_ddl().unwrap()
                {
                    return Err(format!(
                        "unexpected MySQL view DDL: {:?}",
                        view_detail.source_ddl()
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
                if !parent_unique_index.is_unique()
                    || parent_unique_index.columns != ["unique_code"]
                {
                    return Err(format!(
                        "unexpected MySQL parent unique index: {parent_unique_index:?}"
                    ));
                }
                let parent_unique_column = parent
                    .columns
                    .iter()
                    .find(|column| column.name == "unique_code")
                    .ok_or_else(|| "MySQL unique column was not returned".to_string())?;
                if !parent_unique_column.is_unique() {
                    return Err(format!(
                        "unexpected MySQL unique column attributes: {parent_unique_column:?}"
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
    async fn reflects_single_column_unique_metadata_in_columns_and_signatures() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let composite_index = "uq_mysql_metadata_child_composite";
                let single_index = "uq_mysql_metadata_child_parent_first";
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "ALTER TABLE {MYSQL_FK_CHILD} ADD UNIQUE KEY {composite_index} (payload(10), parent_first)"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to add composite unique index: {error:?}"))?;

                let composite = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FK_CHILD)
                    .await
                    .map_err(|error| format!("failed to fetch composite unique metadata: {error:?}"))?;
                if composite
                    .columns
                    .iter()
                    .filter(|column| {
                        ["parent_first", "payload"].contains(&column.name.as_str())
                    })
                    .any(sabiql_domain::Column::is_unique)
                {
                    return Err(format!(
                        "composite unique index marked a column unique: {composite:?}"
                    ));
                }
                if !composite
                    .indexes
                    .iter()
                    .any(|index| index.name == composite_index && index.is_unique())
                {
                    return Err(format!(
                        "composite unique index was not displayed: {composite:?}"
                    ));
                }
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("ALTER TABLE {MYSQL_FK_CHILD} DROP INDEX {composite_index}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to drop composite unique index: {error:?}"))?;

                let before = db
                    .adapter()
                    .fetch_table_signatures(db.dsn())
                    .await
                    .map_err(|error| format!("failed to fetch baseline signatures: {error:?}"))?;
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "ALTER TABLE {MYSQL_FK_CHILD} ADD UNIQUE KEY {single_index} (parent_first)"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to add single-column unique index: {error:?}"))?;

                let detail = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FK_CHILD)
                    .await
                    .map_err(|error| format!("failed to fetch single unique metadata: {error:?}"))?;
                let parent_first = detail
                    .columns
                    .iter()
                    .find(|column| column.name == "parent_first")
                    .ok_or_else(|| "single unique column was not returned".to_string())?;
                if !parent_first.is_unique() {
                    return Err(format!(
                        "single unique column was not marked: {parent_first:?}"
                    ));
                }

                let light = db
                    .adapter()
                    .fetch_table_columns_and_fks(db.dsn(), "sabiql_test", MYSQL_FK_CHILD)
                    .await
                    .map_err(|error| format!("failed to fetch light metadata: {error:?}"))?;
                let light_parent_first = light
                    .columns
                    .iter()
                    .find(|column| column.name == "parent_first")
                    .ok_or_else(|| "light metadata omitted single unique column".to_string())?;
                if !light_parent_first.is_unique() || !light.indexes.is_empty() {
                    return Err(format!("unexpected light unique metadata: {light:?}"));
                }

                let after_add = db
                    .adapter()
                    .fetch_table_signatures(db.dsn())
                    .await
                    .map_err(|error| format!("failed to fetch changed signatures: {error:?}"))?;
                if before == after_add {
                    return Err("single unique index did not change the table signature".to_string());
                }

                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("ALTER TABLE {MYSQL_FK_CHILD} DROP INDEX {single_index}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to drop single-column unique index: {error:?}"))?;
                let after_drop = db
                    .adapter()
                    .fetch_table_signatures(db.dsn())
                    .await
                    .map_err(|error| format!("failed to fetch restored signatures: {error:?}"))?;
                if before != after_drop {
                    return Err("dropping the single unique index did not restore the signature".to_string());
                }
                Ok(())
            })
        })
        .await;
    }
}

mod query_preview {

    use super::shared::{MYSQL_COMPOSITE_TABLE, MYSQL_EMPTY_TABLE, MYSQL_VIEW};
    use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, with_mysql_test_db};
    use sabiql_app::ports::outbound::{AccessMode, QueryExecutor};
    use sabiql_domain::QueryValue;

    const MYSQL_NO_PK_TABLE: &str = "mysql_preview_no_pk";
    const MYSQL_SPATIAL_TABLE: &str = "demo_warehouses";

    fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
        if value.is_empty() || !value.len().is_multiple_of(2) {
            return Err(format!("invalid HEX value: {value}"));
        }
        (0..value.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&value[index..index + 2], 16)
                    .map_err(|error| format!("invalid HEX value: {value}: {error}"))
            })
            .collect()
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
    async fn previews_mysql_spatial_point_as_binary_equal_to_hex() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let hex_result = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "SELECT HEX(location) FROM demo_warehouses WHERE code = 'TYO001'",
                        AccessMode::ReadOnly,
                    )
                    .await
                    .map_err(|error| format!("failed to read spatial HEX value: {error:?}"))?;
                let hex = match hex_result.values().first().and_then(|row| row.first()) {
                    Some(QueryValue::Text(value)) => value,
                    value => return Err(format!("unexpected spatial HEX result: {value:?}")),
                };
                let expected = decode_hex(hex)?;

                let preview = db
                    .adapter()
                    .execute_preview(db.dsn(), "sabiql_test", MYSQL_SPATIAL_TABLE, 1, 0)
                    .await
                    .map_err(|error| format!("failed to preview spatial table: {error:?}"))?;
                let location_index = preview
                    .columns
                    .iter()
                    .position(|column| column == "location")
                    .ok_or_else(|| format!("spatial column missing from preview: {preview:?}"))?;
                let value = preview
                    .values()
                    .first()
                    .and_then(|row| row.get(location_index))
                    .ok_or_else(|| format!("spatial value missing from preview: {preview:?}"))?;
                if value != &QueryValue::Blob(expected) {
                    return Err(format!("unexpected spatial preview value: {value:?}"));
                }
                Ok::<(), String>(())
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
}

mod write_operations {

    use crate::tests::harness::mysql::with_mysql_test_db;
    use sabiql_app::ports::outbound::{AccessMode, MetadataProvider, QueryExecutor, SqlDialect};
    use sabiql_domain::{CommandTag, DatabaseType, QueryValue};

    const MYSQL_INVISIBLE_PK_TABLE: &str = "mysql_edit_invisible_pk";
    const MYSQL_INVISIBLE_COMPOSITE_TABLE: &str = "mysql_edit_invisible_composite";
    const MYSQL_GIPK_TABLE: &str = "mysql_edit_gipk";
    const MYSQL_FUNCTIONAL_INDEX: &str = "mysql_metadata_functional";

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn executes_supported_mysql_ddl_forms_on_oracle_mysql_84() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let suffix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| format!("system clock error: {error}"))?
                    .as_nanos();
                let table = format!("sabiql_ddl_{suffix}");
                let renamed_table = format!("sabiql_ddl_renamed_{suffix}");
                let view = format!("sabiql_ddl_view_{suffix}");
                let result: Result<(), String> = async {
                    let create = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!(
                                "CREATE TABLE {table} (id INT PRIMARY KEY, body TEXT NOT NULL) /*!40100 DEFAULT CHARSET=utf8mb4 */"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to create version-comment table: {error:?}"))?;
                    if create.command_tag != Some(CommandTag::Create("TABLE".to_string())) {
                        return Err(format!("unexpected CREATE TABLE result: {create:?}"));
                    }

                    let rename = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("RENAME TABLE {table} TO {renamed_table}"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to rename table: {error:?}"))?;
                    if rename.command_tag != Some(CommandTag::Alter("TABLE".to_string())) {
                        return Err(format!("unexpected RENAME TABLE result: {rename:?}"));
                    }

                    let create_view = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!(
                                "CREATE OR REPLACE ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW {view} AS SELECT id, body FROM {renamed_table}"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to create or replace view: {error:?}"))?;
                    if create_view.command_tag != Some(CommandTag::Create("VIEW".to_string())) {
                        return Err(format!(
                            "unexpected CREATE OR REPLACE VIEW result: {create_view:?}"
                        ));
                    }

                    let alter_view = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!(
                                "ALTER ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW {view} AS SELECT id, body FROM {renamed_table}"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to alter view: {error:?}"))?;
                    if alter_view.command_tag != Some(CommandTag::Alter("VIEW".to_string())) {
                        return Err(format!("unexpected ALTER VIEW result: {alter_view:?}"));
                    }

                    let create_fulltext = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!(
                                "CREATE FULLTEXT INDEX {view}_body ON {renamed_table} (body)"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to create fulltext index: {error:?}"))?;
                    if create_fulltext.command_tag != Some(CommandTag::Create("INDEX".to_string())) {
                        return Err(format!(
                            "unexpected CREATE FULLTEXT INDEX result: {create_fulltext:?}"
                        ));
                    }

                    let view_result = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("SELECT id, body FROM {view}"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to query altered view: {error:?}"))?;
                    if view_result.columns != ["id", "body"] || !view_result.values().is_empty() {
                        return Err(format!("unexpected DDL view result: {view_result:?}"));
                    }

                    let drop = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("DROP TABLE {renamed_table} /*!80000 RESTRICT */"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to drop table with version comment: {error:?}"))?;
                    if drop.command_tag != Some(CommandTag::Drop("TABLE".to_string())) {
                        return Err(format!("unexpected DROP TABLE result: {drop:?}"));
                    }
                    Ok(())
                }
                .await;

                let cleanup = db
                    .run_cli_script(&format!(
                        "DROP VIEW IF EXISTS {view}; DROP TABLE IF EXISTS {renamed_table}; DROP TABLE IF EXISTS {table}"
                    ))
                    .await;
                match (result, cleanup) {
                    (Ok(()), Ok(_)) => Ok(()),
                    (Err(error), Ok(_)) => Err(error),
                    (Ok(()), Err(error)) => Err(format!("DDL cleanup failed: {error}")),
                    (Err(error), Err(cleanup_error)) => {
                        Err(format!("{error}; DDL cleanup failed: {cleanup_error}"))
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn loads_and_updates_mysql_functional_index_table() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let detail = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_FUNCTIONAL_INDEX)
                    .await
                    .map_err(|error| {
                        format!("failed to fetch functional index metadata: {error:?}")
                    })?;

                let functional = detail
                    .indexes
                    .iter()
                    .find(|index| index.name == "uq_mysql_metadata_functional_json")
                    .ok_or_else(|| "MySQL functional index was not returned".to_string())?;
                if !functional.is_unique()
                    || !functional.has_expression()
                    || functional.columns.len() != 1
                    || functional.definition.is_none()
                {
                    return Err(format!("unexpected MySQL functional index: {functional:?}"));
                }

                let mixed = detail
                    .indexes
                    .iter()
                    .find(|index| index.name == "idx_mysql_metadata_functional_mixed")
                    .ok_or_else(|| "MySQL mixed functional index was not returned".to_string())?;
                if !mixed.has_expression()
                    || mixed.columns.len() != 2
                    || mixed.columns[1] != "sort_key"
                    || mixed.definition.is_none()
                {
                    return Err(format!("unexpected MySQL mixed index: {mixed:?}"));
                }

                let preview = db
                    .adapter()
                    .execute_preview(db.dsn(), "sabiql_test", MYSQL_FUNCTIONAL_INDEX, 10, 0)
                    .await
                    .map_err(|error| {
                        format!("failed to preview functional index table: {error:?}")
                    })?;
                if preview.columns != ["id", "payload", "sort_key"]
                    || preview.values()
                        != [[
                            QueryValue::SqlLiteral("1".to_string()),
                            QueryValue::Text("{\"code\": \"A-001\"}".to_string()),
                            QueryValue::SqlLiteral("10".to_string()),
                        ]]
                {
                    return Err(format!("unexpected functional index preview: {preview:?}"));
                }

                let update_sql = db.adapter().build_update_sql(
                    DatabaseType::MySQL,
                    "sabiql_test",
                    MYSQL_FUNCTIONAL_INDEX,
                    "sort_key",
                    &QueryValue::SqlLiteral("20".to_string()),
                    &[("id".to_string(), QueryValue::SqlLiteral("1".to_string()))],
                );
                let update = db
                    .adapter()
                    .execute_write(db.dsn(), &update_sql, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| {
                        format!("failed to update functional index table: {error:?}")
                    })?;
                if update.affected_rows != 1 {
                    return Err(format!("unexpected functional index update: {update:?}"));
                }

                Ok::<(), String>(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn previews_and_writes_rows_using_hidden_mysql_primary_keys() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let mut gipk_identity_value = None;
                let result = async {
                    let invisible = db
                    .adapter()
                    .execute_preview(db.dsn(), "sabiql_test", MYSQL_INVISIBLE_PK_TABLE, 10, 0)
                    .await
                    .map_err(|error| format!("failed to preview invisible primary key: {error:?}"))?;
                    if invisible.columns != ["payload"]
                        || invisible.query.contains("__sabiql_row_identity_")
                        || invisible.values()
                            != [[QueryValue::Text("invisible single primary key".to_string())]]
                    {
                        return Err(format!(
                            "unexpected invisible primary key preview: {invisible:?}"
                        ));
                    }
                    let identity = invisible
                        .explicit_row_identity()
                        .ok_or_else(|| "invisible primary key identity was not returned".to_string())?;
                    if identity.columns() != ["id"]
                        || identity.values() != [[QueryValue::SqlLiteral("1".to_string())]]
                    {
                        return Err(format!(
                            "unexpected invisible primary key identity: {identity:?}"
                        ));
                    }

                    let composite = db
                    .adapter()
                    .execute_preview(
                        db.dsn(),
                        "sabiql_test",
                        MYSQL_INVISIBLE_COMPOSITE_TABLE,
                        10,
                        0,
                    )
                    .await
                    .map_err(|error| format!("failed to preview invisible composite key: {error:?}"))?;
                let composite_identity = composite
                    .explicit_row_identity()
                    .ok_or_else(|| "invisible composite identity was not returned".to_string())?;
                    if composite.columns != ["payload"]
                    || composite.query.contains("__sabiql_row_identity_")
                    || composite_identity.columns() != ["second_key", "first_key"]
                    || composite_identity.values()
                        != [[
                            QueryValue::SqlLiteral("20".to_string()),
                            QueryValue::SqlLiteral("1".to_string()),
                        ]]
                    {
                        return Err(format!(
                            "unexpected invisible composite preview: {composite:?}"
                        ));
                    }

                    let gipk_detail = db
                    .adapter()
                    .fetch_table_detail(db.dsn(), "sabiql_test", MYSQL_GIPK_TABLE)
                    .await
                    .map_err(|error| format!("failed to fetch GIPK metadata: {error:?}"))?;
                    if gipk_detail.primary_key != Some(vec!["my_row_id".to_string()]) {
                        return Err(format!(
                            "GIPK was not exposed by INFORMATION_SCHEMA: {:?}",
                            gipk_detail.primary_key
                        ));
                    }
                    let gipk = db
                    .adapter()
                    .execute_preview(db.dsn(), "sabiql_test", MYSQL_GIPK_TABLE, 10, 0)
                    .await
                    .map_err(|error| format!("failed to preview GIPK: {error:?}"))?;
                    if gipk.columns != ["payload"]
                    || gipk.query.contains("__sabiql_row_identity_")
                    || gipk
                        .explicit_row_identity()
                        .is_none_or(|identity| identity.columns() != ["my_row_id"])
                    {
                        return Err(format!("unexpected GIPK preview: {gipk:?}"));
                    }
                    let gipk_identity = gipk
                        .explicit_row_identity()
                        .ok_or_else(|| "GIPK row identity was not returned".to_string())?;
                    let identity_value = gipk_identity
                        .values_for_row(0)
                        .and_then(|values| values.first())
                        .cloned()
                        .ok_or_else(|| "GIPK row identity value was not returned".to_string())?;
                    gipk_identity_value = Some(identity_value.clone());

                    let update_sql = db.adapter().build_update_sql(
                        DatabaseType::MySQL,
                        "sabiql_test",
                        MYSQL_GIPK_TABLE,
                        "payload",
                        &QueryValue::text("updated through GIPK"),
                        &[("my_row_id".to_string(), identity_value.clone())],
                    );
                    if !update_sql.contains("WHERE `my_row_id` =")
                        || update_sql.contains("SET `my_row_id` =")
                    {
                        return Err(format!("unexpected GIPK update SQL: {update_sql}"));
                    }
                    let update = db
                        .adapter()
                        .execute_write(db.dsn(), &update_sql, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| format!("failed to update through GIPK: {error:?}"))?;
                    if update.affected_rows != 1 {
                        return Err(format!("unexpected GIPK update result: {update:?}"));
                    }
                    let changed = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            "SELECT my_row_id, payload FROM mysql_edit_gipk",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to read updated GIPK row: {error:?}"))?;
                    if changed.values()
                        != [[
                            QueryValue::text(identity_value.display_value()),
                            QueryValue::text("updated through GIPK"),
                        ]]
                    {
                        return Err(format!("unexpected updated GIPK row: {changed:?}"));
                    }

                    let delete_sql = db.adapter().build_bulk_delete_sql(
                        DatabaseType::MySQL,
                        "sabiql_test",
                        MYSQL_INVISIBLE_PK_TABLE,
                        &[vec![(
                            "id".to_string(),
                            QueryValue::SqlLiteral("1".to_string()),
                        )]],
                    );
                    let delete = db
                        .adapter()
                        .execute_write(db.dsn(), &delete_sql, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| {
                            format!("failed to delete through invisible primary key: {error:?}")
                        })?;
                    if delete.affected_rows != 1 {
                        return Err(format!(
                            "unexpected invisible primary key delete result: {delete:?}"
                        ));
                    }
                    Ok::<(), String>(())
                }
                .await;

                let restore_gipk: Result<(), String> = match gipk_identity_value.as_ref() {
                    Some(identity_value) => {
                        let restore_sql = db.adapter().build_update_sql(
                            DatabaseType::MySQL,
                            "sabiql_test",
                            MYSQL_GIPK_TABLE,
                            "payload",
                            &QueryValue::text("generated invisible primary key"),
                            &[("my_row_id".to_string(), identity_value.clone())],
                        );
                        db.adapter()
                            .execute_adhoc(db.dsn(), &restore_sql, AccessMode::ReadWrite)
                            .await
                            .map(|_| ())
                            .map_err(|error| format!("failed to restore GIPK fixture: {error:?}"))
                    }
                    None => Ok(()),
                };
                let restore_invisible = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "INSERT INTO mysql_edit_invisible_pk (id, payload) VALUES (1, 'invisible single primary key')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to restore invisible primary key fixture: {error:?}"));

                match result {
                    Err(error) => Err(error),
                    Ok(()) => restore_gipk.and_then(|()| restore_invisible.map(|_| ())),
                }
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
                let result = async {
                    db.adapter()
                        .execute_adhoc(db.dsn(), &create, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| format!("failed to create write fixture: {error:?}"))?;
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

                let drop = format!("DROP TABLE IF EXISTS {table}");
                let cleanup = db
                    .adapter()
                    .execute_adhoc(db.dsn(), &drop, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| format!("failed to clean up write fixture: {error:?}"));
                match result {
                    Err(error) => Err(error),
                    Ok(()) => cleanup.map(|_| ()),
                }
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn updates_mysql_json_documents_as_whole_values() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let suffix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| format!("system clock error: {error}"))?
                    .as_nanos();
                let table = format!("mysql_sab396_{suffix}");
                let create = format!(
                    "CREATE TABLE {table} (id INT NOT NULL PRIMARY KEY, payload JSON NOT NULL)"
                );
                let result = async {
                    db.adapter()
                        .execute_adhoc(db.dsn(), &create, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| format!("failed to create JSON fixture: {error:?}"))?;
                    db.adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("INSERT INTO {table} VALUES (1, '{{\"a\":1}}')"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to seed JSON fixture: {error:?}"))?;

                    let update_json = db.adapter().build_update_sql(
                        DatabaseType::MySQL,
                        "sabiql_test",
                        &table,
                        "payload",
                        &QueryValue::text(r#"{"b":2,"a":1}"#),
                        &[("id".to_string(), QueryValue::SqlLiteral("1".to_string()))],
                    );
                    if update_json.contains("JSON_SET") {
                        return Err("JSON update unexpectedly used JSON_SET".to_string());
                    }
                    db.adapter()
                        .execute_write(db.dsn(), &update_json, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| format!("failed to update JSON document: {error:?}"))?;

                    let reordered = db
                        .adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("SELECT JSON_EXTRACT(payload, '$.b'), JSON_EXTRACT(payload, '$.a') FROM {table} WHERE id = 1"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("failed to read JSON document: {error:?}"))?;
                    if reordered.values()
                        != [[QueryValue::Text("2".to_string()), QueryValue::Text("1".to_string())]]
                    {
                        return Err(format!("unexpected updated JSON document: {reordered:?}"));
                    }

                    for (json, expected_type) in [("null", "NULL"), (r#""null""#, "STRING")] {
                        let update = db.adapter().build_update_sql(
                            DatabaseType::MySQL,
                            "sabiql_test",
                            &table,
                            "payload",
                            &QueryValue::text(json),
                            &[("id".to_string(), QueryValue::SqlLiteral("1".to_string()))],
                        );
                        db.adapter()
                            .execute_write(db.dsn(), &update, AccessMode::ReadWrite)
                            .await
                            .map_err(|error| format!("failed to update JSON null value: {error:?}"))?;
                        let value = db
                            .adapter()
                            .execute_adhoc(
                                db.dsn(),
                                &format!("SELECT JSON_TYPE(payload) FROM {table} WHERE id = 1"),
                                AccessMode::ReadWrite,
                            )
                            .await
                            .map_err(|error| format!("failed to read JSON type: {error:?}"))?;
                        if value.values() != [[QueryValue::Text(expected_type.to_string())]] {
                            return Err(format!("unexpected JSON type: {value:?}"));
                        }
                    }
                    Ok(())
                }
                .await;

                let drop = format!("DROP TABLE IF EXISTS {table}");
                let cleanup = db
                    .adapter()
                    .execute_adhoc(db.dsn(), &drop, AccessMode::ReadWrite)
                    .await
                    .map_err(|error| format!("failed to clean up JSON fixture: {error:?}"));
                match result {
                    Err(error) => Err(error),
                    Ok(()) => cleanup.map(|_| ()),
                }
            })
        })
        .await;
    }
}

mod query_execution {

    use super::shared::MYSQL_EMPTY_TABLE;
    use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, with_mysql_test_db};
    use sabiql_app::ports::outbound::{
        AccessMode, DbOperationError, QueryExecutor, UnsupportedOperationKind,
    };
    use sabiql_domain::{CommandTag, QueryValue, RefreshScope};
    use sabiql_infra::adapters::mysql::execute_mysql_adhoc_with_read_only_session_for_test;
    use std::time::Duration;

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn preserves_xml_value_boundaries_for_real_mysql_results() {
        with_mysql_test_db(|db| Box::pin(async move {
            let result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!(
                        "SELECT nullable_text, empty_text, unicode_text, JSON_EXTRACT(json_value, '$.array'), JSON_EXTRACT(json_value, '$.text'), blob_value, CONVERT(CONCAT('line one', CHAR(10), 'ERROR 1146 (42S02): not a CLI error') USING utf8mb4), 'x|y', 'first\\nmiddle\\nlast', 'tail\\t ', CAST(NULL AS CHAR(8)), 'NULL' FROM {MYSQL_FIXTURE_TABLE}"
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
                QueryValue::Text("line one\nERROR 1146 (42S02): not a CLI error".to_string()),
                QueryValue::Text("x|y".to_string()),
                QueryValue::Text("first\nmiddle\nlast".to_string()),
                QueryValue::Text("tail\t ".to_string()),
                QueryValue::Null,
                QueryValue::Text("NULL".to_string()),
            ];
            if result.values() != [expected] {
                return Err(format!("unexpected XML values: {:?}", result.values()));
            }
            Ok(())
        }))
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn preserves_empty_select_columns_without_reexecution() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let select = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    "SELECT 1 AS first_alias, '' AS empty_alias, '日本語' AS unicode_alias WHERE FALSE",
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("empty SELECT failed: {error:?}"))?;
                if select.columns != ["first_alias", "empty_alias", "unicode_alias"]
                    || !select.values().is_empty()
                {
                    return Err(format!("unexpected empty SELECT result: {select:?}"));
                }

                let cte = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "WITH cte_rows AS (SELECT 1 AS first_alias) SELECT first_alias FROM cte_rows WHERE FALSE",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty CTE SELECT failed: {error:?}"))?;
                if cte.columns != ["first_alias"] || !cte.values().is_empty() {
                    return Err(format!("unexpected empty CTE result: {cte:?}"));
                }

                let cte_with_columns = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "WITH cte_rows(first_alias) AS (SELECT 1) SELECT first_alias FROM cte_rows WHERE FALSE",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty CTE column-list SELECT failed: {error:?}"))?;
                if cte_with_columns.columns != ["first_alias"]
                    || !cte_with_columns.values().is_empty()
                {
                    return Err(format!(
                        "unexpected empty CTE column-list result: {cte_with_columns:?}"
                    ));
                }

                let trailing_comment = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "SELECT 1 AS value WHERE FALSE -- trailing comment",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty SELECT with trailing comment failed: {error:?}"))?;
                if trailing_comment.columns != ["value"] || !trailing_comment.values().is_empty() {
                    return Err(format!(
                        "unexpected empty SELECT with trailing comment: {trailing_comment:?}"
                    ));
                }

                let case_expression = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "SELECT CASE (1) WHEN 1 THEN 'x' ELSE 'y' END AS value WHERE FALSE",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty CASE SELECT failed: {error:?}"))?;
                if case_expression.columns != ["value"] || !case_expression.values().is_empty() {
                    return Err(format!(
                        "unexpected empty CASE SELECT result: {case_expression:?}"
                    ));
                }

                for (query, expected_column) in [
                    ("SELECT CONCAT('a', 'b') AS concatenated WHERE FALSE", "concatenated"),
                    ("SELECT CAST(1 AS CHAR) AS cast_value WHERE FALSE", "cast_value"),
                    ("SELECT @sabiql_metadata_value AS read_value WHERE FALSE", "read_value"),
                ] {
                    let result = db
                        .adapter()
                        .execute_adhoc(db.dsn(), query, AccessMode::ReadWrite)
                        .await
                        .map_err(|error| format!("empty SELECT metadata fallback failed: {error:?}"))?;
                    if result.columns != [expected_column] || !result.values().is_empty() {
                        return Err(format!("unexpected empty SELECT result: {result:?}"));
                    }
                }

                let non_evaluated = tokio::time::timeout(
                    Duration::from_secs(5),
                    db.adapter().execute_adhoc(
                        db.dsn(),
                        "SELECT SLEEP(10) AS sleep_value WHERE FALSE",
                        AccessMode::ReadWrite,
                    ),
                )
                .await
                .map_err(|_| "empty SELECT metadata fallback evaluated SLEEP".to_string())?
                .map_err(|error| format!("empty SELECT non-evaluation proof failed: {error:?}"))?;
                if non_evaluated.columns != ["sleep_value"] || !non_evaluated.values().is_empty() {
                    return Err(format!(
                        "unexpected non-evaluation proof result: {non_evaluated:?}"
                    ));
                }

                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn rejects_unsafe_empty_selects_before_execution() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let duplicate_aliases = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "SELECT 1 AS duplicate_alias, 2 AS duplicate_alias WHERE FALSE",
                        AccessMode::ReadWrite,
                    )
                    .await;
                if !matches!(
                    duplicate_aliases,
                    Err(DbOperationError::UnsupportedOperation(ref details))
                        if details.contains("duplicate column names")
                ) {
                    return Err(format!(
                        "duplicate aliases were not rejected safely: {duplicate_aliases:?}"
                    ));
                }

                for query in [
                    "SELECT @sabiql_metadata_value := 1 AS assigned_value WHERE FALSE",
                    "SELECT GET_LOCK('sabiql_metadata_lock', 0) AS lock_value WHERE FALSE",
                    "SELECT id FROM mysql_cli_fixture WHERE FALSE FOR UPDATE",
                ] {
                    let result = db
                        .adapter()
                        .execute_adhoc(db.dsn(), query, AccessMode::ReadWrite)
                        .await;
                    if !matches!(result, Err(DbOperationError::UnsupportedOperation(_))) {
                        return Err(format!(
                            "unsafe empty SELECT was not rejected: {query}: {result:?}"
                        ));
                    }
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn preserves_empty_show_columns() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let show = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "SHOW TABLES LIKE 'sabiql_empty_metadata_missing'",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty SHOW failed: {error:?}"))?;
                if show.columns != ["Tables_in_sabiql_test (sabiql_empty_metadata_missing)"]
                    || !show.values().is_empty()
                {
                    return Err(format!("unexpected empty SHOW result: {show:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn preserves_empty_describe_columns() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let describe = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("DESCRIBE {MYSQL_EMPTY_TABLE} 'missing_column'"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty DESCRIBE failed: {error:?}"))?;
                if describe.columns != ["Field", "Type", "Null", "Key", "Default", "Extra"]
                    || !describe.values().is_empty()
                {
                    return Err(format!("unexpected empty DESCRIBE result: {describe:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn preserves_empty_table_columns() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let table = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("TABLE {MYSQL_EMPTY_TABLE}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("empty TABLE failed: {error:?}"))?;
                if table.columns != ["id", "payload"] || !table.values().is_empty() {
                    return Err(format!("unexpected empty TABLE result: {table:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn executes_multiple_statements_and_returns_only_the_last_result() {
        with_mysql_test_db(|db| Box::pin(async move {
            let result = async {
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
                Ok::<(), String>(())
            }
            .await;
            let cleanup = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = '' WHERE id = 1"),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("failed to restore fixture: {error:?}"));
            match result {
                Err(error) => Err(error),
                Ok(()) => cleanup.map(|_| ()),
            }
        }))
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn rejects_multiple_table_update_and_delete_before_mysql_cli_execution() {
        with_mysql_test_db(|db| Box::pin(async move {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("system clock error: {error}"))?
                .as_nanos();
            let target_table = format!("mysql_sab451_target_{suffix}");
            let source_table = format!("mysql_sab451_source_{suffix}");
            let result = async {
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "CREATE TABLE {target_table} (id INT PRIMARY KEY, value INT); CREATE TABLE {source_table} (id INT PRIMARY KEY, value INT); INSERT INTO {target_table} VALUES (1, 1); INSERT INTO {source_table} VALUES (1, 2)"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to create multi-table fixtures: {error:?}"))?;

                for query in [
                    format!(
                        "UPDATE {target_table} AS target JOIN {source_table} AS source ON target.id = source.id SET target.value = source.value"
                    ),
                    format!(
                        "DELETE target FROM {target_table} AS target JOIN {source_table} AS source ON target.id = source.id"
                    ),
                ] {
                    let result = db
                        .adapter()
                        .execute_adhoc(db.dsn(), &query, AccessMode::ReadWrite)
                        .await;
                    if !matches!(
                        result,
                        Err(DbOperationError::UnsupportedOperation(ref details))
                            if details.contains("multiple-table")
                    ) {
                        return Err(format!(
                            "multiple-table mutation was not rejected safely: {query}: {result:?}"
                        ));
                    }
                }

                let result = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("SELECT value FROM {target_table} WHERE id = 1"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to verify multi-table fixture: {error:?}"))?;
                if result.values() != [[QueryValue::Text("1".to_string())]] {
                    return Err(format!(
                        "multi-table mutation changed the fixture: {result:?}"
                    ));
                }
                Ok::<(), String>(())
            }
            .await;
            let cleanup = async {
                for table in [&target_table, &source_table] {
                    db.adapter()
                        .execute_adhoc(
                            db.dsn(),
                            &format!("DROP TABLE IF EXISTS {table}"),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| {
                            format!("failed to clean up multi-table fixture {table}: {error:?}")
                        })?;
                }
                Ok::<(), String>(())
            };
            match result {
                Err(error) => {
                    cleanup.await?;
                    Err(error)
                }
                Ok(()) => cleanup.await,
            }
        }))
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn keeps_refresh_scope_and_discards_partial_result_after_a_later_failure() {
        with_mysql_test_db(|db| Box::pin(async move {
            let result = async {
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
                Ok::<(), String>(())
            }
            .await;
            let cleanup = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = '' WHERE id = 1"),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("failed to restore fixture: {error:?}"));
            match result {
                Err(error) => Err(error),
                Ok(()) => cleanup.map(|_| ()),
            }
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

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn rejects_implicit_commit_transaction_and_matches_oracle_mysql_behavior() {
        with_mysql_test_db(|db| Box::pin(async move {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("system clock error: {error}"))?
                .as_nanos();
            let table = format!("sabiql_sab439_{suffix}");
            let query = format!(
                "BEGIN; UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'implicit commit' WHERE id = 1; CREATE TABLE {table} (id INT); ROLLBACK"
            );
            let validation = db
                .adapter()
                .execute_adhoc(db.dsn(), &query, AccessMode::ReadWrite)
                .await;
            if !matches!(
                validation,
                Err(DbOperationError::UnsupportedOperation(ref details))
                    if details.contains("implicit commit")
            ) {
                return Err(format!("implicit-commit transaction was not rejected: {validation:?}"));
            }

            let result = async {
                db.run_cli_script(&format!(
                    "BEGIN; UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'implicit commit' WHERE id = 1; CREATE TABLE {table} (id INT); ROLLBACK"
                ))
                .await
                .map_err(|error| format!("raw MySQL implicit-commit check failed: {error}"))?;

                let updated = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("SELECT empty_text FROM {MYSQL_FIXTURE_TABLE} WHERE id = 1"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to read the committed update: {error:?}"))?;
                let table_exists = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = '{table}'"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to read the committed DDL: {error:?}"))?;
                if updated.values() != [[QueryValue::Text("implicit commit".to_string())]]
                    || table_exists.values() != [[QueryValue::Text("1".to_string())]]
                {
                    return Err(format!(
                        "Oracle MySQL did not preserve the implicit commit: updated={updated:?}, table_exists={table_exists:?}"
                    ));
                }
                Ok::<(), String>(())
            }
            .await;
            let cleanup = async {
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("DROP TABLE IF EXISTS {table}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to clean up implicit-commit table: {error:?}"))?;
                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = '' WHERE id = 1"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to restore fixture: {error:?}"))?;
                Ok::<(), String>(())
            };
            match result {
                Err(error) => {
                    cleanup.await?;
                    Err(error)
                }
                Ok(()) => cleanup.await,
            }
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
                || result.refresh_scope != RefreshScope::Data
            {
                return Err(format!("unexpected temporary-table result: {result:?}"));
            }

            let ddl_only_table = format!("sabiql_sab461_tmp_{suffix}");
            let ddl_only_result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!(
                        "CREATE TEMPORARY TABLE {ddl_only_table} (id INT); DROP TEMPORARY TABLE {ddl_only_table}"
                    ),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("temporary-table DDL-only query failed: {error:?}"))?;
            if ddl_only_result.command_tag
                    != Some(CommandTag::Other("DROP TEMPORARY TABLE".to_string()))
                || ddl_only_result.refresh_scope != RefreshScope::None
            {
                return Err(format!(
                    "unexpected temporary-table DDL-only result: {ddl_only_result:?}"
                ));
            }

            let empty_table = format!("sabiql_sab414_empty_tmp_{suffix}");
            let empty_result = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!(
                        "CREATE TEMPORARY TABLE {empty_table} (id INT); SELECT id FROM {empty_table} WHERE FALSE; DROP TEMPORARY TABLE {empty_table}"
                    ),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("empty temporary-table query failed: {error:?}"))?;
            if empty_result.columns != ["id"] || !empty_result.values().is_empty() {
                return Err(format!("unexpected empty temporary-table result: {empty_result:?}"));
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
                let result = if matches!(
                    result.as_ref(),
                    Err(DbOperationError::QueryFailedAfterChange {
                        refresh_scope: RefreshScope::Metadata,
                        ..
                    })
                ) {
                    Ok(())
                } else {
                    Err(format!("expected metadata-scope failure: {result:?}"))
                };
                let cleanup = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("DROP TABLE IF EXISTS {table}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to clean up DDL fixture: {error:?}"));
                match result {
                    Err(error) => Err(error),
                    Ok(()) => cleanup.map(|_| ()),
                }
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
            if !matches!(result, Err(DbOperationError::ObjectMissing(ref details)) if details.contains("missing_column")) {
                return Err(format!("expected a query failure without a result: {result:?}"));
            }
            Ok(())
        }))
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn classifies_real_mysql_server_errors_by_server_code() {
        with_mysql_test_db(|db| Box::pin(async move {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("system clock error: {error}"))?
                .as_nanos();
            let lock_table = format!("sabiql_mrf2_05_lock_{suffix}");
            let missing_table = format!("sabiql_mrf2_05_missing_{suffix}");
            let result = async {
                let missing = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("SELECT id FROM {missing_table}"),
                        AccessMode::ReadWrite,
                    )
                    .await;
                if !matches!(missing, Err(DbOperationError::ObjectMissing(_))) {
                    return Err(format!("missing object was not classified: {missing:?}"));
                }

                let duplicate = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        "CREATE TEMPORARY TABLE sabiql_mrf2_05_duplicate (id INT PRIMARY KEY); INSERT INTO sabiql_mrf2_05_duplicate VALUES (1); INSERT INTO sabiql_mrf2_05_duplicate VALUES (1)",
                        AccessMode::ReadWrite,
                    )
                    .await;
                if !matches!(
                    &duplicate,
                    Err(DbOperationError::QueryFailedAfterChange { source, .. })
                        if matches!(&**source, DbOperationError::UniqueViolation(_))
                ) {
                    return Err(format!("duplicate error was not classified: {duplicate:?}"));
                }

                db.adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "CREATE TABLE {lock_table} (id INT PRIMARY KEY, value INT NOT NULL); INSERT INTO {lock_table} VALUES (1, 0), (2, 0)"
                        ),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("failed to create lock fixture: {error:?}"))?;

                let first_query = format!(
                    "BEGIN; UPDATE {lock_table} SET value = value + 1 WHERE id = 1; SELECT SLEEP(1); UPDATE {lock_table} SET value = value + 1 WHERE id = 2; COMMIT"
                );
                let second_query = format!(
                    "BEGIN; UPDATE {lock_table} SET value = value + 1 WHERE id = 2; SELECT SLEEP(1); UPDATE {lock_table} SET value = value + 1 WHERE id = 1; COMMIT"
                );
                let first = db.adapter().execute_adhoc(
                    db.dsn(),
                    &first_query,
                    AccessMode::ReadWrite,
                );
                let second = db.adapter().execute_adhoc(
                    db.dsn(),
                    &second_query,
                    AccessMode::ReadWrite,
                );
                let (first, second) = tokio::join!(first, second);
                let lock_error = match (first, second) {
                    (Err(error), _) | (_, Err(error)) => Some(error),
                    (Ok(_), Ok(_)) => None,
                };
                match lock_error.as_ref() {
                    Some(error) if error.summary() == "Operation blocked by lock or timeout" => {}
                    _ => {
                        return Err(format!(
                            "deadlock error was not classified: {lock_error:?}"
                        ));
                    }
                }

                Ok::<(), String>(())
            }
            .await;
            let cleanup = db
                .adapter()
                .execute_adhoc(
                    db.dsn(),
                    &format!("DROP TABLE IF EXISTS {lock_table}"),
                    AccessMode::ReadWrite,
                )
                .await
                .map_err(|error| format!("failed to clean up lock fixture: {error:?}"));
            match result {
                Err(error) => {
                    cleanup?;
                    Err(error)
                }
                Ok(()) => cleanup.map(|_| ()),
            }
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
    async fn applies_read_only_session_before_adhoc_sql_and_preserves_fixture() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let result = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!(
                            "SELECT @@SESSION.transaction_read_only AS transaction_read_only, id, empty_text FROM {MYSQL_FIXTURE_TABLE}"
                        ),
                        AccessMode::ReadOnly,
                    )
                    .await
                    .map_err(|error| format!("read-only adhoc query failed: {error:?}"))?;
                if result.columns != ["transaction_read_only", "id", "empty_text"]
                    || result.values()
                        != [[
                            QueryValue::Text("1".to_string()),
                            QueryValue::Text("1".to_string()),
                            QueryValue::Text(String::new()),
                        ]]
                {
                    return Err(format!("unexpected read-only adhoc result: {result:?}"));
                }

                let write = execute_mysql_adhoc_with_read_only_session_for_test(
                    db.dsn(),
                    &format!(
                        "UPDATE {MYSQL_FIXTURE_TABLE} SET empty_text = 'read-only mutation' WHERE id = 1"
                    ),
                )
                    .await;
                if !matches!(write, Err(DbOperationError::QueryFailed(ref details)) if details.contains("READ ONLY"))
                {
                    return Err(format!("read-only adhoc write was not rejected: {write:?}"));
                }

                let fixture = db
                    .adapter()
                    .execute_adhoc(
                        db.dsn(),
                        &format!("SELECT id, empty_text FROM {MYSQL_FIXTURE_TABLE}"),
                        AccessMode::ReadWrite,
                    )
                    .await
                    .map_err(|error| format!("fixture verification failed: {error:?}"))?;
                if fixture.values()
                    != [[QueryValue::Text("1".to_string()), QueryValue::Text(String::new())]]
                {
                    return Err(format!("read-only write changed the fixture: {fixture:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn rejects_next_process_when_global_sql_mode_is_unsupported() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
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
                        .execute_adhoc(db.dsn(), "SELECT SLEEP(32)", AccessMode::ReadWrite)
                        .await;
                    if !matches!(
                        result,
                        Err(DbOperationError::UnsupportedOperationWithKind {
                            kind: UnsupportedOperationKind::SessionMode,
                            ..
                        })
                    ) {
                        return Err(format!(
                            "expected unsupported sql_mode rejection: {result:?}"
                        ));
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
            })
        })
        .await;
    }
}

mod csv_export {

    use super::shared::MYSQL_EMPTY_TABLE;
    use crate::tests::harness::mysql::{MYSQL_FIXTURE_TABLE, with_mysql_test_db};
    use sabiql_app::ports::outbound::{DbOperationError, QueryExecutor};
    use sabiql_infra::adapters::mysql::export_mysql_csv_to_path_for_test;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and CLI"]
    async fn exports_with_a_read_only_session_and_rejects_writes() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let output_directory = tempdir().map_err(|error| error.to_string())?;
                let path = output_directory.path().join("read_only.csv");
                let path = export_mysql_csv_to_path_for_test(
                    db.dsn(),
                    "SELECT @@SESSION.transaction_read_only AS transaction_read_only",
                    path,
                )
                .await
                .map_err(|error| format!("read-only CSV export failed: {error:?}"))?;
                let csv = std::fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read CSV export: {error}"))?;
                if csv != "transaction_read_only\n1\n" {
                    return Err(format!("unexpected read-only CSV export: {csv:?}"));
                }

                let write_path = output_directory.path().join("write.csv");
                let result = export_mysql_csv_to_path_for_test(
                    db.dsn(),
                    &format!("INSERT INTO {MYSQL_FIXTURE_TABLE} (id) VALUES (2)"),
                    write_path.clone(),
                )
                .await;
                if !matches!(
                    result,
                    Err(DbOperationError::QueryFailed(ref details))
                        if details.contains("READ ONLY")
                ) {
                    return Err(format!("write export was not rejected: {result:?}"));
                }
                if write_path.exists() {
                    return Err("write export created an output file".to_string());
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn exports_each_supported_mysql_result_statement() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let output_directory = tempdir().map_err(|error| error.to_string())?;
                for (name, query) in [
                    ("select", format!("SELECT id FROM {MYSQL_FIXTURE_TABLE}")),
                    ("table", format!("TABLE {MYSQL_EMPTY_TABLE}")),
                    ("show", format!("SHOW TABLES LIKE '{MYSQL_FIXTURE_TABLE}'")),
                    ("describe", format!("DESCRIBE {MYSQL_FIXTURE_TABLE}")),
                ] {
                    let path = export_mysql_csv_to_path_for_test(
                        db.dsn(),
                        &query,
                        output_directory.path().join(format!("{name}.csv")),
                    )
                    .await
                    .map_err(|error| format!("{name} CSV export failed: {error:?}"))?;
                    let csv = std::fs::read_to_string(&path)
                        .map_err(|error| format!("failed to read {name} CSV export: {error}"))?;
                    if csv.trim().is_empty() {
                        return Err(format!("{name} CSV export was empty"));
                    }
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn count_query_rows_rejects_empty_and_non_integer_results() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let empty = db
                    .adapter()
                    .count_query_rows(
                        db.dsn(),
                        &format!("SELECT id FROM {MYSQL_EMPTY_TABLE} WHERE FALSE"),
                    )
                    .await;
                if !matches!(
                    empty,
                    Err(DbOperationError::QueryFailed(ref details))
                        if details.contains("invalid result")
                ) {
                    return Err(format!("empty count result was accepted: {empty:?}"));
                }

                let non_integer = db
                    .adapter()
                    .count_query_rows(db.dsn(), "SELECT 'not-a-count'")
                    .await;
                if !matches!(
                    non_integer,
                    Err(DbOperationError::QueryFailed(ref details))
                        if details.contains("not an integer")
                ) {
                    return Err(format!(
                        "non-integer count result was accepted: {non_integer:?}"
                    ));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[cfg(unix)]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn exports_resultset_field_error_text_verbatim_through_real_mysql_cli() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let output_directory = tempdir().map_err(|error| error.to_string())?;
                let path = export_mysql_csv_to_path_for_test(
                    db.dsn(),
                    "SELECT CONVERT(CONCAT('line 1', CHAR(10), 'ERROR 1146 (42S02): this is a cell value') USING utf8mb4) AS message",
                    output_directory.path().join("field-error.csv"),
                )
                .await
                .map_err(|error| format!("field-error CSV export failed: {error:?}"))?;
                let csv = std::fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read field-error CSV export: {error}"))?;
                let expected =
                    "message\n\"line 1\nERROR 1146 (42S02): this is a cell value\"\n";
                if csv != expected {
                    return Err(format!("unexpected field-error CSV export: {csv:?}"));
                }
                Ok(())
            })
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires Oracle MySQL 8.4 server and mysql CLI"]
    async fn exports_a_header_only_csv_for_an_empty_result() {
        with_mysql_test_db(|db| {
            Box::pin(async move {
                let output_directory = tempdir().map_err(|error| error.to_string())?;
                let path = export_mysql_csv_to_path_for_test(
                    db.dsn(),
                    "SELECT 1 AS first_alias, '' AS empty_alias WHERE FALSE",
                    output_directory.path().join("empty.csv"),
                )
                .await
                .map_err(|error| format!("empty CSV export failed: {error:?}"))?;
                let csv = std::fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read empty CSV export: {error}"))?;
                if csv != "first_alias,empty_alias\n" {
                    return Err(format!("unexpected empty CSV export: {csv:?}"));
                }

                let table_path = export_mysql_csv_to_path_for_test(
                    db.dsn(),
                    &format!("TABLE {MYSQL_EMPTY_TABLE}"),
                    output_directory.path().join("empty-table.csv"),
                )
                .await
                .map_err(|error| format!("empty TABLE CSV export failed: {error:?}"))?;
                let table_csv = std::fs::read_to_string(&table_path)
                    .map_err(|error| format!("failed to read empty TABLE CSV export: {error}"))?;
                if table_csv != "id,payload\n" {
                    return Err(format!("unexpected empty TABLE CSV export: {table_csv:?}"));
                }
                Ok(())
            })
        })
        .await;
    }
}
