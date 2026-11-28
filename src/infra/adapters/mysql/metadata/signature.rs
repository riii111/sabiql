use crate::domain::connection::MySqlConnectionConfig;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{Table, TableKindInfo, TableSignature, TableSignatureSnapshot};

use super::super::cli::{MYSQL_QUERY_TIMEOUT, MySqlResultSet};
use super::super::dsn::NormalizedMySqlConfig;
use super::super::sql::{
    FOREIGN_KEY_RESULT_COLUMNS, SIGNATURE_COLUMNS_QUERY, SIGNATURE_COLUMNS_RESULT_COLUMNS,
    SIGNATURE_FOREIGN_KEYS_QUERY, SIGNATURE_UNIQUE_COLUMNS_QUERY,
    SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS, TABLE_SIGNATURES_QUERY,
    TABLE_SIGNATURES_RESULT_COLUMNS, TABLES_QUERY, TABLES_RESULT_COLUMNS,
};
use super::catalog::{
    MySqlColumnMetadata, MySqlForeignKeyMetadata, MySqlTableMetadata, column_from_metadata,
    execute_metadata_queries_in_session_with_program, expect_columns, foreign_keys_from_metadata,
    mark_single_column_unique, metadata_shape_error, metadata_snapshot_from_result,
    parse_column_metadata_row, parse_foreign_key_metadata, primary_key_names, required_text,
    selected_database,
};

#[derive(Debug, Clone)]
struct MySqlSignatureColumnMetadata {
    schema: String,
    table: String,
    column: MySqlColumnMetadata,
}

pub(super) async fn fetch_table_signatures(
    config: &MySqlConnectionConfig,
) -> Result<TableSignatureSnapshot, DbOperationError> {
    fetch_table_signatures_with_program(config, OsStr::new("mysql"), MYSQL_QUERY_TIMEOUT).await
}

async fn fetch_table_signatures_with_program(
    config: &MySqlConnectionConfig,
    program: &OsStr,
    timeout: Duration,
) -> Result<TableSignatureSnapshot, DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    selected_database(&target)?;
    let (_, results) = execute_metadata_queries_in_session_with_program(
        &target,
        &[(TABLE_SIGNATURES_QUERY, TABLE_SIGNATURES_RESULT_COLUMNS)],
        program,
        timeout,
    )
    .await?;
    parse_table_signatures(&results[0])
}

pub(super) async fn fetch_table_details(
    config: &MySqlConnectionConfig,
) -> Result<Vec<Table>, DbOperationError> {
    fetch_details_with_program(config, OsStr::new("mysql"), MYSQL_QUERY_TIMEOUT).await
}

async fn fetch_details_with_program(
    config: &MySqlConnectionConfig,
    program: &OsStr,
    timeout: Duration,
) -> Result<Vec<Table>, DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    let database = selected_database(&target)?;
    let (capabilities, results) = execute_metadata_queries_in_session_with_program(
        &target,
        &[
            (TABLES_QUERY, TABLES_RESULT_COLUMNS),
            (SIGNATURE_COLUMNS_QUERY, SIGNATURE_COLUMNS_RESULT_COLUMNS),
            (SIGNATURE_FOREIGN_KEYS_QUERY, FOREIGN_KEY_RESULT_COLUMNS),
            (
                SIGNATURE_UNIQUE_COLUMNS_QUERY,
                SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS,
            ),
        ],
        program,
        timeout,
    )
    .await?;
    let lower_case_table_names = capabilities.lower_case_table_names;
    let tables =
        metadata_snapshot_from_result(database, None, &results[0], lower_case_table_names)?;
    table_details_from_metadata(
        &tables,
        database,
        lower_case_table_names,
        parse_signature_column_metadata(&results[1])?,
        parse_foreign_key_metadata(&results[2])?,
        parse_signature_unique_column_metadata(&results[3])?,
    )
}

fn parse_signature_column_metadata(
    result: &MySqlResultSet,
) -> Result<Vec<MySqlSignatureColumnMetadata>, DbOperationError> {
    expect_columns(result, SIGNATURE_COLUMNS_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != SIGNATURE_COLUMNS_RESULT_COLUMNS.len() {
                return Err(metadata_shape_error("signature COLUMNS row"));
            }
            Ok(MySqlSignatureColumnMetadata {
                schema: required_text(&row[0], "TABLE_SCHEMA")?.to_string(),
                table: required_text(&row[1], "TABLE_NAME")?.to_string(),
                column: parse_column_metadata_row(&row[2..], "signature COLUMNS row")?,
            })
        })
        .collect()
}

fn table_details_from_metadata(
    tables: &[MySqlTableMetadata],
    database: &str,
    lower_case_table_names: u8,
    columns: Vec<MySqlSignatureColumnMetadata>,
    foreign_keys: Vec<MySqlForeignKeyMetadata>,
    mut unique_columns_by_table: HashMap<String, HashSet<String>>,
) -> Result<Vec<Table>, DbOperationError> {
    let known_tables: HashSet<(String, String)> = tables
        .iter()
        .map(|table| (table.schema.clone(), table.name.clone()))
        .collect();
    let mut columns_by_table: HashMap<(String, String), Vec<MySqlColumnMetadata>> = HashMap::new();
    for metadata in columns {
        let key = (metadata.schema.clone(), metadata.table.clone());
        if !known_tables.contains(&key) {
            return Err(DbOperationError::MetadataParseFailed(format!(
                "MySQL signature metadata references unknown table: {}.{}",
                metadata.schema, metadata.table
            )));
        }
        columns_by_table
            .entry(key)
            .or_default()
            .push(metadata.column);
    }

    let mut foreign_keys_by_table: HashMap<(String, String), Vec<MySqlForeignKeyMetadata>> =
        HashMap::new();
    for foreign_key in foreign_keys {
        let key = (
            foreign_key.from_schema.clone(),
            foreign_key.from_table.clone(),
        );
        if !known_tables.contains(&key) {
            return Err(DbOperationError::MetadataParseFailed(format!(
                "MySQL signature metadata references unknown table: {}.{}",
                foreign_key.from_schema, foreign_key.from_table
            )));
        }
        foreign_keys_by_table
            .entry(key)
            .or_default()
            .push(foreign_key);
    }

    if unique_columns_by_table
        .keys()
        .any(|name| !tables.iter().any(|table| table.name == *name))
    {
        return Err(metadata_shape_error(
            "signature UNIQUE metadata references unknown table",
        ));
    }

    let mut details = Vec::with_capacity(tables.len());
    for table in tables {
        let key = (table.schema.clone(), table.name.clone());
        let mut columns = columns_by_table.remove(&key).ok_or_else(|| {
            DbOperationError::MetadataParseFailed(format!(
                "MySQL object has no column metadata: {}.{}",
                table.schema, table.name
            ))
        })?;
        if columns.is_empty() {
            return Err(DbOperationError::MetadataParseFailed(format!(
                "MySQL object has no column metadata: {}.{}",
                table.schema, table.name
            )));
        }
        columns.sort_by_key(|column| column.ordinal_position);
        let unique_columns = unique_columns_by_table
            .remove(&table.name)
            .unwrap_or_default();
        mark_single_column_unique(&mut columns, &unique_columns);
        let primary_key = primary_key_names(&columns);
        let foreign_keys = foreign_keys_from_metadata(
            foreign_keys_by_table.remove(&key).unwrap_or_default(),
            database,
            lower_case_table_names,
        )?;
        let detail = Table {
            schema: table.schema.clone(),
            name: table.name.clone(),
            owner: None,
            columns: columns.iter().map(column_from_metadata).collect(),
            primary_key: (!primary_key.is_empty()).then_some(primary_key),
            foreign_keys,
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            storage_attributes: table.storage_attributes(),
            kind_info: TableKindInfo {
                kind: table.kind,
                is_strict: false,
                without_rowid: false,
                virtual_module: None,
            },
        };
        details.push(detail);
    }
    Ok(details)
}

fn parse_table_signatures(
    result: &MySqlResultSet,
) -> Result<TableSignatureSnapshot, DbOperationError> {
    expect_columns(result, TABLE_SIGNATURES_RESULT_COLUMNS)?;
    let signatures = result
        .values
        .iter()
        .map(|row| {
            if row.len() != TABLE_SIGNATURES_RESULT_COLUMNS.len() {
                return Err(metadata_shape_error("table signature row"));
            }
            Ok(TableSignature {
                schema: required_text(&row[0], "TABLE_SCHEMA")?.to_string(),
                name: required_text(&row[1], "TABLE_NAME")?.to_string(),
                signature: required_text(&row[2], "SIGNATURE")?.to_string(),
            })
        })
        .collect::<Result<_, DbOperationError>>()?;
    Ok(TableSignatureSnapshot { signatures })
}

fn parse_signature_unique_column_metadata(
    result: &MySqlResultSet,
) -> Result<HashMap<String, HashSet<String>>, DbOperationError> {
    expect_columns(result, SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS)?;
    let mut unique_columns_by_table = HashMap::new();
    for row in &result.values {
        if row.len() != SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS.len() {
            return Err(metadata_shape_error("signature UNIQUE row"));
        }
        unique_columns_by_table
            .entry(required_text(&row[0], "TABLE_NAME")?.to_string())
            .or_insert_with(HashSet::new)
            .insert(required_text(&row[1], "COLUMN_NAME")?.to_string());
    }
    Ok(unique_columns_by_table)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::result;
    use super::*;
    use crate::domain::{QueryValue, TableKind};

    #[test]
    fn signature_metadata_batches_columns_and_foreign_keys_by_server_table() {
        let tables = vec![
            MySqlTableMetadata {
                schema: "App".to_string(),
                name: "child".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
                engine: Some("InnoDB".to_string()),
                row_format: Some("Dynamic".to_string()),
                table_collation: Some("utf8mb4_0900_ai_ci".to_string()),
                create_options: Some("partitioned".to_string()),
            },
            MySqlTableMetadata {
                schema: "App".to_string(),
                name: "parent".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
                engine: None,
                row_format: None,
                table_collation: None,
                create_options: None,
            },
        ];
        let columns = parse_signature_column_metadata(&result(
            &[
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "COLUMN_NAME",
                "COLUMN_TYPE",
                "IS_NULLABLE",
                "COLUMN_DEFAULT",
                "EXTRA",
                "COLUMN_COMMENT",
                "ORDINAL_POSITION",
                "PRIMARY_KEY_POSITION",
            ],
            vec![
                vec![
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
                vec![
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("2".to_string()),
                ],
                vec![
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
            ],
        ))
        .unwrap();
        let foreign_keys = parse_foreign_key_metadata(&result(
            &[
                "CONSTRAINT_NAME",
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "COLUMN_NAME",
                "REFERENCED_TABLE_SCHEMA",
                "REFERENCED_TABLE_NAME",
                "REFERENCED_COLUMN_NAME",
                "ORDINAL_POSITION",
                "UPDATE_RULE",
                "DELETE_RULE",
            ],
            vec![vec![
                QueryValue::Text("fk_child_parent".to_string()),
                QueryValue::Text("App".to_string()),
                QueryValue::Text("child".to_string()),
                QueryValue::Text("parent_id".to_string()),
                QueryValue::Text("App".to_string()),
                QueryValue::Text("parent".to_string()),
                QueryValue::Text("id".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Text("CASCADE".to_string()),
                QueryValue::Text("SET NULL".to_string()),
            ]],
        ))
        .unwrap();

        let unique_columns = parse_signature_unique_column_metadata(&result(
            SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS,
            vec![vec![
                QueryValue::Text("child".to_string()),
                QueryValue::Text("parent_id".to_string()),
            ]],
        ))
        .unwrap();
        let details = table_details_from_metadata(
            &tables,
            "App",
            0,
            columns.clone(),
            foreign_keys.clone(),
            unique_columns,
        )
        .unwrap();
        assert_eq!(details.len(), 2);
        let child = &details[0];
        assert_eq!(child.columns.len(), 2);
        assert!(child.columns[1].is_unique());
        assert_eq!(child.foreign_keys.len(), 1);
        assert!(child.indexes.is_empty());
        assert!(child.triggers.is_empty());
        assert!(child.row_count_estimate.is_none());
        assert!(child.comment.is_none());
        assert_eq!(child.storage_attributes.engine.as_deref(), Some("InnoDB"));
    }

    #[test]
    fn signature_metadata_requires_columns_for_each_table() {
        let tables = vec![MySqlTableMetadata {
            schema: "app".to_string(),
            name: "users".to_string(),
            kind: TableKind::Table,
            row_count_estimate: None,
            comment: None,
            engine: None,
            row_format: None,
            table_collation: None,
            create_options: None,
        }];
        let error =
            table_details_from_metadata(&tables, "app", 0, Vec::new(), Vec::new(), HashMap::new())
                .unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::MetadataParseFailed(message)
                if message.contains("no column metadata: app.users")
        ));
    }
}

#[cfg(all(test, unix))]
mod session_tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::super::test_support::{assert_process_stopped, connection_config};
    use super::*;

    fn fake_signature_cli() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("mysql-signature");
        let transcript = directory.path().join("transcript.log");
        std::fs::write(&transcript, "").unwrap();
        let script = r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
transcript=$(dirname "$0")/transcript.log
printf 'option=%s\nprocess=%s\n' "$option" "$$" >> "$transcript"
trap 'printf "exit=%s\n" "$?" >> "$transcript"' EXIT
eof=$(printf '\004')
while IFS= read -r line; do
  printf 'query=%s\n' "$line" >> "$transcript"
  [ "$line" = "$eof" ] && exit 0
  [ "$line" = ";" ] && continue
    case "$line" in
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_probe.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_server_version">8.4.10</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
      ;;
    *"SET SESSION TRANSACTION READ ONLY")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_session_marker.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    *COLUMNS*)
      printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="COLUMN_NAME">id</field><field name="COLUMN_TYPE">int</field><field name="IS_NULLABLE">NO</field><field name="COLUMN_DEFAULT" xsi:nil="true"/><field name="EXTRA"></field><field name="COLUMN_COMMENT" xsi:nil="true"/><field name="ORDINAL_POSITION">1</field><field name="PRIMARY_KEY_POSITION">1</field></row></resultset>'
      ;;
    *REFERENTIAL_CONSTRAINTS*)
      printf '%s\n' '<resultset></resultset>'
      ;;
    *"GROUP BY s.TABLE_NAME"*)
      printf '%s\n' '<resultset></resultset>'
      ;;
    *TABLES*)
      printf '%s\n' '<resultset><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="TABLE_TYPE">BASE TABLE</field><field name="TABLE_ROWS">1</field><field name="TABLE_COMMENT">table comment</field><field name="ENGINE">InnoDB</field><field name="ROW_FORMAT">Dynamic</field><field name="TABLE_COLLATION">utf8mb4_0900_ai_ci</field><field name="CREATE_OPTIONS">partitioned</field></row></resultset>'
      ;;
    *)
      printf '%s\n' '<resultset></resultset>'
      ;;
  esac
done
"#;
        std::fs::write(&program, script).unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&program, permissions).unwrap();
        (directory, program, transcript)
    }

    #[tokio::test]
    async fn batch_metadata_uses_one_process_and_four_metadata_queries() {
        let (_directory, program, transcript) = fake_signature_cli();
        let details = fetch_details_with_program(
            &connection_config("user", "password", "app"),
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "fake signature metadata CLI failed: {error:?}\n{}",
                std::fs::read_to_string(&transcript).unwrap()
            )
        });

        assert_eq!(details.len(), 1);
        let transcript_text = std::fs::read_to_string(&transcript).unwrap();
        assert_eq!(
            transcript_text
                .lines()
                .filter(|line| line.starts_with("process="))
                .count(),
            1
        );
        assert_eq!(
            transcript_text
                .lines()
                .filter(|line| {
                    line.starts_with("query=") && line.contains("INFORMATION_SCHEMA")
                })
                .count(),
            4
        );
        assert_process_stopped(&transcript);
    }
}
