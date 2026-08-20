use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    FkAction, Table, TableKind, TableKindInfo, TableSignature, TableSignatureSnapshot,
};

use super::super::cli::{MYSQL_QUERY_TIMEOUT, MySqlResultSet};
use super::super::sql::{
    FOREIGN_KEY_RESULT_COLUMNS, SIGNATURE_COLUMNS_QUERY, SIGNATURE_COLUMNS_RESULT_COLUMNS,
    SIGNATURE_FOREIGN_KEYS_QUERY, SIGNATURE_UNIQUE_COLUMNS_QUERY,
    SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS, TABLES_QUERY, TABLES_RESULT_COLUMNS,
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
    dsn: &str,
) -> Result<TableSignatureSnapshot, DbOperationError> {
    fetch_table_signatures_with_program(dsn, OsStr::new("mysql"), MYSQL_QUERY_TIMEOUT).await
}

async fn fetch_table_signatures_with_program(
    dsn: &str,
    program: &OsStr,
    timeout: Duration,
) -> Result<TableSignatureSnapshot, DbOperationError> {
    let database = selected_database(dsn)?;
    let (lower_case_table_names, results) = execute_metadata_queries_in_session_with_program(
        dsn,
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
    let snapshot =
        metadata_snapshot_from_result(&database, None, &results[0], lower_case_table_names)?;
    table_signatures_from_metadata(
        &snapshot.tables,
        &database,
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
            if row.len() != 10 {
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

fn table_signatures_from_metadata(
    tables: &[MySqlTableMetadata],
    database: &str,
    lower_case_table_names: u8,
    columns: Vec<MySqlSignatureColumnMetadata>,
    foreign_keys: Vec<MySqlForeignKeyMetadata>,
    mut unique_columns_by_table: HashMap<String, HashSet<String>>,
) -> Result<TableSignatureSnapshot, DbOperationError> {
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

    let mut signatures = Vec::with_capacity(tables.len());
    let mut table_details = Vec::with_capacity(tables.len());
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
            kind_info: TableKindInfo {
                kind: table.kind,
                is_strict: false,
                without_rowid: false,
                virtual_module: None,
            },
        };
        signatures.push(TableSignature {
            schema: table.schema.clone(),
            name: table.name.clone(),
            signature: table_signature(&detail),
        });
        table_details.push(detail);
    }
    Ok(TableSignatureSnapshot {
        signatures,
        table_details,
    })
}

fn table_signature(table: &Table) -> String {
    let mut signature = String::new();
    append_record(&mut signature, "table");
    append_value(&mut signature, &table.schema);
    append_value(&mut signature, &table.name);
    append_value(&mut signature, table_kind_name(table.kind_info.kind));

    append_record(&mut signature, "primary-key");
    match &table.primary_key {
        Some(columns) => {
            append_value(&mut signature, "some");
            append_values(&mut signature, columns);
        }
        None => append_value(&mut signature, "none"),
    }

    let mut columns = table.columns.iter().collect::<Vec<_>>();
    columns.sort_by(|left, right| {
        left.ordinal_position
            .cmp(&right.ordinal_position)
            .then_with(|| left.name.cmp(&right.name))
    });
    for column in columns {
        append_record(&mut signature, "column");
        append_value(&mut signature, &column.name);
        append_value(&mut signature, &column.data_type);
        append_bool(&mut signature, column.is_nullable());
        append_optional_value(&mut signature, column.default.as_deref());
        append_optional_value(&mut signature, column.comment.as_deref());
        append_value(&mut signature, &column.ordinal_position.to_string());
        append_bool(&mut signature, column.is_primary_key());
        append_bool(&mut signature, column.is_unique());
        append_bool(&mut signature, column.is_read_only());
        append_bool(&mut signature, column.is_hidden());
        append_bool(&mut signature, column.is_generated());
    }

    let mut foreign_keys = table.foreign_keys.iter().collect::<Vec<_>>();
    foreign_keys.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.from_schema.cmp(&right.from_schema))
            .then_with(|| left.from_table.cmp(&right.from_table))
            .then_with(|| left.from_columns.cmp(&right.from_columns))
            .then_with(|| left.to_schema.cmp(&right.to_schema))
            .then_with(|| left.to_table.cmp(&right.to_table))
            .then_with(|| left.to_columns.cmp(&right.to_columns))
            .then_with(|| {
                foreign_key_action_name(&left.on_delete)
                    .cmp(foreign_key_action_name(&right.on_delete))
            })
            .then_with(|| {
                foreign_key_action_name(&left.on_update)
                    .cmp(foreign_key_action_name(&right.on_update))
            })
            .then_with(|| left.reference_resolved.cmp(&right.reference_resolved))
    });
    for foreign_key in foreign_keys {
        append_record(&mut signature, "foreign-key");
        append_value(&mut signature, &foreign_key.name);
        append_value(&mut signature, &foreign_key.from_schema);
        append_value(&mut signature, &foreign_key.from_table);
        append_values(&mut signature, &foreign_key.from_columns);
        append_value(&mut signature, &foreign_key.to_schema);
        append_value(&mut signature, &foreign_key.to_table);
        append_values(&mut signature, &foreign_key.to_columns);
        append_value(
            &mut signature,
            foreign_key_action_name(&foreign_key.on_delete),
        );
        append_value(
            &mut signature,
            foreign_key_action_name(&foreign_key.on_update),
        );
        append_bool(&mut signature, foreign_key.reference_resolved);
    }

    signature
}

fn append_record(signature: &mut String, name: &str) {
    signature.push_str(name);
    signature.push('|');
}

fn append_value(signature: &mut String, value: &str) {
    signature.push_str(&value.len().to_string());
    signature.push(':');
    signature.push_str(value);
    signature.push('|');
}

fn append_values(signature: &mut String, values: &[String]) {
    append_value(signature, &values.len().to_string());
    for value in values {
        append_value(signature, value);
    }
}

fn append_optional_value(signature: &mut String, value: Option<&str>) {
    match value {
        Some(value) => {
            append_value(signature, "some");
            append_value(signature, value);
        }
        None => append_value(signature, "none"),
    }
}

fn append_bool(signature: &mut String, value: bool) {
    append_value(signature, if value { "true" } else { "false" });
}

fn table_kind_name(kind: TableKind) -> &'static str {
    match kind {
        TableKind::Table => "table",
        TableKind::Virtual => "virtual",
        TableKind::View => "view",
    }
}

fn foreign_key_action_name(action: &FkAction) -> &'static str {
    match action {
        FkAction::NoAction => "no-action",
        FkAction::Restrict => "restrict",
        FkAction::Cascade => "cascade",
        FkAction::SetNull => "set-null",
        FkAction::SetDefault => "set-default",
    }
}

fn parse_signature_unique_column_metadata(
    result: &MySqlResultSet,
) -> Result<HashMap<String, HashSet<String>>, DbOperationError> {
    expect_columns(result, SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS)?;
    let mut unique_columns_by_table = HashMap::new();
    for row in &result.values {
        if row.len() != 2 {
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
    use super::*;
    use crate::domain::{Column, ColumnAttributes, ForeignKey, QueryValue};

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    fn signature_table() -> Table {
        Table {
            schema: "app".to_string(),
            name: "child".to_string(),
            owner: None,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    default: None,
                    attributes: ColumnAttributes::from_parts(false, true, false),
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "parent_id".to_string(),
                    data_type: "bigint".to_string(),
                    default: Some("0".to_string()),
                    attributes: ColumnAttributes::from_parts(true, false, false),
                    comment: Some("parent reference".to_string()),
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![
                ForeignKey {
                    name: "fk_z_parent".to_string(),
                    from_schema: "app".to_string(),
                    from_table: "child".to_string(),
                    from_columns: vec!["parent_id".to_string()],
                    to_schema: "app".to_string(),
                    to_table: "parents".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::SetNull,
                    on_update: FkAction::Cascade,
                    reference_resolved: true,
                },
                ForeignKey {
                    name: "fk_a_audit".to_string(),
                    from_schema: "app".to_string(),
                    from_table: "child".to_string(),
                    from_columns: vec!["id".to_string()],
                    to_schema: "audit".to_string(),
                    to_table: "entries".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::Restrict,
                    on_update: FkAction::NoAction,
                    reference_resolved: true,
                },
            ],
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            kind_info: TableKindInfo {
                kind: TableKind::Table,
                is_strict: false,
                without_rowid: false,
                virtual_module: None,
            },
        }
    }

    fn assert_signature_changes(change: impl FnOnce(&mut Table)) {
        let mut changed = signature_table();
        let original = table_signature(&changed);
        change(&mut changed);

        assert_ne!(original, table_signature(&changed));
    }

    #[test]
    fn table_signature_does_not_depend_on_debug_output() {
        let table = signature_table();
        let signature = table_signature(&table);

        assert!(signature.contains("fk_a_audit"));
        assert!(!signature.contains("Column {"));
        assert!(!signature.contains("ForeignKey {"));
        assert!(!signature.contains("ColumnAttributes"));

        let mut unrelated = table;
        unrelated.owner = Some("owner".to_string());
        unrelated.comment = Some("display comment".to_string());
        unrelated.source_ddl = Some("CREATE TABLE child (...)".to_string());
        unrelated.row_count_estimate = Some(42);
        unrelated.kind_info.is_strict = true;
        unrelated.kind_info.without_rowid = true;
        unrelated.kind_info.virtual_module = Some("module".to_string());
        assert_eq!(signature, table_signature(&unrelated));
    }

    #[test]
    fn table_signature_is_independent_of_metadata_fetch_order() {
        let mut reordered = signature_table();
        reordered.columns.reverse();
        reordered.foreign_keys.reverse();

        assert_eq!(
            table_signature(&signature_table()),
            table_signature(&reordered)
        );
    }

    #[test]
    fn table_signature_changes_for_semantic_metadata() {
        assert_signature_changes(|table| table.schema = "other".to_string());
        assert_signature_changes(|table| table.name = "other".to_string());
        assert_signature_changes(|table| table.kind_info.kind = TableKind::View);
        assert_signature_changes(|table| table.columns[0].name = "identifier".to_string());
        assert_signature_changes(|table| table.columns[0].data_type = "bigint".to_string());
        assert_signature_changes(|table| {
            table.columns[0].attributes = ColumnAttributes::from_parts(true, true, false);
        });
        assert_signature_changes(|table| table.primary_key = Some(vec!["parent_id".to_string()]));
        assert_signature_changes(|table| table.foreign_keys[0].to_table = "accounts".to_string());
        assert_signature_changes(|table| {
            table.foreign_keys[0].from_columns = vec!["id".to_string()];
        });
        assert_signature_changes(|table| table.foreign_keys[0].on_delete = FkAction::Cascade);
    }

    #[test]
    fn signature_metadata_batches_columns_and_foreign_keys_by_server_table() {
        let tables = vec![
            MySqlTableMetadata {
                schema: "App".to_string(),
                name: "child".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
            },
            MySqlTableMetadata {
                schema: "App".to_string(),
                name: "parent".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
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
        let signatures = table_signatures_from_metadata(
            &tables,
            "App",
            0,
            columns.clone(),
            foreign_keys.clone(),
            unique_columns,
        )
        .unwrap();
        let signatures_without_unique = table_signatures_from_metadata(
            &tables,
            "App",
            0,
            columns,
            foreign_keys,
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(signatures.signatures.len(), 2);
        assert_eq!(signatures.table_details.len(), 2);
        assert_eq!(signatures.signatures[0].qualified_name(), "App.child");
        assert!(signatures.signatures[0].signature.contains("id"));
        assert!(
            signatures.signatures[0]
                .signature
                .contains("fk_child_parent")
        );
        let child = &signatures.table_details[0];
        assert_eq!(child.columns.len(), 2);
        assert!(child.columns[1].is_unique());
        assert_eq!(child.foreign_keys.len(), 1);
        assert!(child.indexes.is_empty());
        assert!(child.triggers.is_empty());
        assert!(child.row_count_estimate.is_none());
        assert!(child.comment.is_none());
        assert_ne!(
            signatures.signatures[0].signature,
            signatures_without_unique.signatures[0].signature
        );
    }

    #[test]
    fn signature_metadata_requires_columns_for_each_table() {
        let tables = vec![MySqlTableMetadata {
            schema: "app".to_string(),
            name: "users".to_string(),
            kind: TableKind::Table,
            row_count_estimate: None,
            comment: None,
        }];
        let error = table_signatures_from_metadata(
            &tables,
            "app",
            0,
            Vec::new(),
            Vec::new(),
            HashMap::new(),
        )
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
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
      ;;
    *"SET SESSION TRANSACTION READ ONLY")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_session_marker.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
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
      printf '%s\n' '<resultset><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="TABLE_TYPE">BASE TABLE</field><field name="TABLE_ROWS">1</field><field name="TABLE_COMMENT">table comment</field></row></resultset>'
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

    fn assert_process_stopped(transcript: &std::path::Path) {
        for _ in 0..200 {
            let pid = std::fs::read_to_string(transcript)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("process=")?.parse::<libc::pid_t>().ok());
            if let Some(pid) = pid
                && unsafe { libc::kill(pid, 0) } == -1
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "fake mysql process is alive or did not start: {}",
            std::fs::read_to_string(transcript).unwrap()
        );
    }

    #[tokio::test]
    async fn signature_metadata_uses_one_process_and_four_metadata_queries() {
        let (_directory, program, transcript) = fake_signature_cli();
        let snapshot = fetch_table_signatures_with_program(
            "mysql://user:password@localhost:3306/app",
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

        assert_eq!(snapshot.signatures.len(), 1);
        assert_eq!(snapshot.table_details.len(), 1);
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
