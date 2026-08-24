use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    ForeignKey, Index, IndexAttributes, IndexType, Table, TableKind, TableKindInfo, Trigger,
    TriggerCreationContext, TriggerEvent, TriggerTiming,
};

use super::super::sql::{
    COLUMN_METADATA_RESULT_COLUMNS, FOREIGN_KEY_RESULT_COLUMNS, INDEX_RESULT_COLUMNS,
    TABLES_RESULT_COLUMNS, TRIGGER_RESULT_COLUMNS, UNIQUE_COLUMN_RESULT_COLUMNS, columns_query,
    foreign_keys_query, indexes_query, show_create_query, show_create_result_columns, table_query,
    triggers_query, unique_columns_query,
};
use super::super::{
    cli::{MYSQL_QUERY_TIMEOUT, MySqlMetadataSession, MySqlResultSet},
    dsn::parse_and_validate_mysql_dsn,
    option_file::MySqlOptionFile,
};
use super::catalog::{
    MySqlColumnMetadata, MySqlTableMetadata, column_from_metadata, expect_columns, find_table,
    foreign_keys_from_metadata, mark_single_column_unique, metadata_shape_error,
    metadata_snapshot_from_result, optional_text, parse_boolean_flag, parse_columns_for_table,
    parse_foreign_key_metadata, parse_optional_positive_i32, parse_positive_i32,
    parse_unique_column_metadata, primary_key_names, required_text, selected_database,
};

#[derive(Debug, Clone)]
struct MySqlIndexMetadata {
    name: String,
    non_unique: bool,
    index_type: String,
    ordinal_position: i32,
    column_name: String,
    sub_part: Option<i32>,
    expression: Option<String>,
    descending: bool,
    visibility: MySqlIndexVisibility,
    primary: bool,
}

#[derive(Debug, Clone, Copy)]
enum MySqlIndexVisibility {
    Visible,
    Invisible,
}

impl MySqlIndexVisibility {
    const fn is_invisible(self) -> bool {
        matches!(self, Self::Invisible)
    }
}

pub(super) async fn fetch_table_detail_in_session(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Table, DbOperationError> {
    fetch_table_detail_in_session_with_program(
        dsn,
        schema,
        table,
        OsStr::new("mysql"),
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn fetch_table_columns_and_fks(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Table, DbOperationError> {
    fetch_table_columns_and_fks_with_program(
        dsn,
        schema,
        table,
        OsStr::new("mysql"),
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

async fn fetch_table_columns_and_fks_with_program(
    dsn: &str,
    schema: &str,
    table: &str,
    program: &OsStr,
    timeout: Duration,
) -> Result<Table, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let database = selected_database(&target)?;
    let table_query = table_query(schema, table);
    let columns_query = columns_query(schema, table);
    let unique_columns_query = unique_columns_query(schema, table);
    let foreign_keys_query = foreign_keys_query(schema, table);
    let (lower_case_table_names, results) =
        super::catalog::execute_metadata_queries_in_session_with_program(
            &target,
            &[
                (table_query.as_str(), TABLES_RESULT_COLUMNS),
                (columns_query.as_str(), COLUMN_METADATA_RESULT_COLUMNS),
                (unique_columns_query.as_str(), UNIQUE_COLUMN_RESULT_COLUMNS),
                (foreign_keys_query.as_str(), FOREIGN_KEY_RESULT_COLUMNS),
            ],
            program,
            timeout,
        )
        .await?;
    let table_metadata =
        table_metadata_from_result(database, schema, table, &results[0], lower_case_table_names)?;
    let mut columns = parse_columns_for_table(&results[1], schema, table)?;
    mark_single_column_unique(&mut columns, &parse_unique_column_metadata(&results[2])?);
    let foreign_keys = foreign_keys_from_metadata(
        parse_foreign_key_metadata(&results[3])?,
        database,
        lower_case_table_names,
    )?;
    Ok(table_from_columns_and_foreign_keys(
        table_metadata,
        columns,
        foreign_keys,
    ))
}

async fn fetch_table_detail_in_session_with_program(
    dsn: &str,
    schema: &str,
    table: &str,
    program: &OsStr,
    timeout: Duration,
) -> Result<Table, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let database = selected_database(&target)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut session = MySqlMetadataSession::spawn_with_metadata_program(program, option_file)?;
    let result = tokio::time::timeout(
        timeout,
        fetch_table_detail_with_session(&mut session, database, schema, table),
    )
    .await;
    session.resolve_timed_result(result).await
}

async fn fetch_table_detail_with_session(
    session: &mut MySqlMetadataSession,
    database: &str,
    schema: &str,
    table: &str,
) -> Result<Table, DbOperationError> {
    let lower_case_table_names = session.prepare_read_only_and_probe().await?;
    let tables_result = session
        .execute_with_expected_columns(&table_query(schema, table), TABLES_RESULT_COLUMNS)
        .await?;
    let table_metadata = table_metadata_from_result(
        database,
        schema,
        table,
        &tables_result,
        lower_case_table_names,
    )?;

    let mut columns = parse_columns_for_table(
        &session
            .execute_with_expected_columns(
                &columns_query(schema, table),
                COLUMN_METADATA_RESULT_COLUMNS,
            )
            .await?,
        schema,
        table,
    )?;
    let raw_indexes = parse_index_metadata(
        &session
            .execute_with_expected_columns(&indexes_query(schema, table), INDEX_RESULT_COLUMNS)
            .await?,
    )?;
    let unique_single_columns = unique_single_columns_from_metadata(&raw_indexes);
    mark_single_column_unique(&mut columns, &unique_single_columns);
    let indexes = indexes_from_metadata(raw_indexes);
    let foreign_keys = foreign_keys_from_metadata(
        parse_foreign_key_metadata(
            &session
                .execute_with_expected_columns(
                    &foreign_keys_query(schema, table),
                    FOREIGN_KEY_RESULT_COLUMNS,
                )
                .await?,
        )?,
        database,
        lower_case_table_names,
    )?;
    let triggers = parse_trigger_metadata(
        &session
            .execute_with_expected_columns(&triggers_query(schema, table), TRIGGER_RESULT_COLUMNS)
            .await?,
    )?;
    let source_ddl = parse_source_ddl(
        &session
            .execute_show_create_with_completion_marker(
                &show_create_query(table, table_metadata.kind),
                show_create_result_columns(table_metadata.kind),
            )
            .await?,
        table_metadata.kind,
    )?;

    let mut detail = table_from_columns_and_foreign_keys(table_metadata, columns, foreign_keys);
    detail.indexes = indexes;
    detail.triggers = triggers;
    detail.source_ddl = Some(source_ddl);
    Ok(detail)
}

fn table_metadata_from_result(
    database: &str,
    schema: &str,
    table: &str,
    result: &MySqlResultSet,
    lower_case_table_names: u8,
) -> Result<MySqlTableMetadata, DbOperationError> {
    metadata_snapshot_from_result(database, Some(schema), result, lower_case_table_names)
        .and_then(|tables| find_table(schema, table, &tables, lower_case_table_names))
}

fn table_from_columns_and_foreign_keys(
    table_metadata: MySqlTableMetadata,
    columns: Vec<MySqlColumnMetadata>,
    foreign_keys: Vec<ForeignKey>,
) -> Table {
    let primary_key = primary_key_names(&columns);
    let storage_attributes = table_metadata.storage_attributes();
    Table {
        schema: table_metadata.schema,
        name: table_metadata.name,
        owner: None,
        columns: columns.iter().map(column_from_metadata).collect(),
        primary_key: (!primary_key.is_empty()).then_some(primary_key),
        foreign_keys,
        indexes: Vec::new(),
        rls: None,
        triggers: Vec::new(),
        row_count_estimate: table_metadata.row_count_estimate,
        comment: table_metadata.comment,
        source_ddl: None,
        storage_attributes,
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            is_strict: false,
            without_rowid: false,
            virtual_module: None,
        },
    }
}

fn parse_trigger_metadata(result: &MySqlResultSet) -> Result<Vec<Trigger>, DbOperationError> {
    expect_columns(result, TRIGGER_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != TRIGGER_RESULT_COLUMNS.len() {
                return Err(metadata_shape_error("TRIGGERS row"));
            }
            let timing = required_text(&row[2], "ACTION_TIMING")?
                .parse::<TriggerTiming>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            let event = required_text(&row[3], "EVENT_MANIPULATION")?
                .parse::<TriggerEvent>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            Ok(Trigger {
                name: required_text(&row[0], "TRIGGER_NAME")?.to_string(),
                timing,
                events: vec![event],
                action_order: Some(parse_positive_i32(&row[1], "ACTION_ORDER")?),
                definition: required_text(&row[4], "ACTION_STATEMENT")?.to_string(),
                security_context: optional_text(&row[5], "DEFINER")?.map(str::to_string),
                creation_context: Some(TriggerCreationContext {
                    sql_mode: optional_text(&row[6], "SQL_MODE")?.map(str::to_string),
                    character_set_client: optional_text(&row[7], "CHARACTER_SET_CLIENT")?
                        .map(str::to_string),
                    collation_connection: optional_text(&row[8], "COLLATION_CONNECTION")?
                        .map(str::to_string),
                    database_collation: optional_text(&row[9], "DATABASE_COLLATION")?
                        .map(str::to_string),
                    created: optional_text(&row[10], "CREATED")?.map(str::to_string),
                }),
            })
        })
        .collect()
}

fn parse_source_ddl(result: &MySqlResultSet, kind: TableKind) -> Result<String, DbOperationError> {
    let expected_columns = if kind == TableKind::View {
        ["View", "Create View"]
    } else {
        ["Table", "Create Table"]
    };
    if result.columns.len() < 2
        || result.columns[0] != expected_columns[0]
        || result.columns[1] != expected_columns[1]
        || result.values.len() != 1
    {
        return Err(metadata_shape_error("SHOW CREATE result"));
    }
    let row = &result.values[0];
    if row.len() != result.columns.len() {
        return Err(metadata_shape_error("SHOW CREATE row"));
    }
    let ddl = required_text(&row[1], expected_columns[1])?;
    if ddl.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL SHOW CREATE returned empty DDL".to_string(),
        ));
    }
    Ok(ddl.to_string())
}

fn parse_index_metadata(
    result: &MySqlResultSet,
) -> Result<Vec<MySqlIndexMetadata>, DbOperationError> {
    expect_columns(result, INDEX_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != INDEX_RESULT_COLUMNS.len() {
                return Err(metadata_shape_error("STATISTICS row"));
            }
            let column_name = optional_text(&row[4], "COLUMN_NAME")?;
            let sub_part = parse_optional_positive_i32(&row[5], "SUB_PART")?;
            let expression = optional_text(&row[6], "EXPRESSION")?;
            let descending = match optional_text(&row[7], "COLLATION")? {
                None => false,
                Some(value) if value.eq_ignore_ascii_case("A") => false,
                Some(value) if value.eq_ignore_ascii_case("D") => true,
                Some(_) => {
                    return Err(DbOperationError::MetadataParseFailed(
                        "invalid MySQL metadata collation".to_string(),
                    ));
                }
            };
            let visibility = if parse_boolean_flag(&row[8], "IS_VISIBLE")? {
                MySqlIndexVisibility::Visible
            } else {
                MySqlIndexVisibility::Invisible
            };
            let (column_name, expression) = match (column_name, expression) {
                (Some(column_name), None) => (column_name.to_string(), None),
                (None, Some(expression)) => {
                    let expression = expression.to_string();
                    (expression.clone(), Some(expression))
                }
                (None, None) => {
                    return Err(DbOperationError::MetadataParseFailed(
                        "MySQL metadata key part has neither COLUMN_NAME nor EXPRESSION"
                            .to_string(),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(DbOperationError::MetadataParseFailed(
                        "MySQL metadata key part has both COLUMN_NAME and EXPRESSION".to_string(),
                    ));
                }
            };
            Ok(MySqlIndexMetadata {
                name: required_text(&row[0], "INDEX_NAME")?.to_string(),
                non_unique: parse_boolean_flag(&row[1], "NON_UNIQUE")?,
                index_type: required_text(&row[2], "INDEX_TYPE")?.to_string(),
                ordinal_position: parse_positive_i32(&row[3], "SEQ_IN_INDEX")?,
                column_name,
                sub_part,
                expression,
                descending,
                visibility,
                primary: parse_boolean_flag(&row[9], "IS_PRIMARY")?,
            })
        })
        .collect()
}

fn indexes_from_metadata(mut raw: Vec<MySqlIndexMetadata>) -> Vec<Index> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut indexes = Vec::new();
    for column in raw {
        let column_name = index_column_name(&column);
        if let Some(index) = indexes
            .iter_mut()
            .find(|index: &&mut Index| index.name == column.name)
        {
            index.columns.push(column_name);
            if let Some(expression) = column.expression {
                index.attributes = index.attributes | IndexAttributes::EXPRESSION;
                index.definition = Some(match index.definition.take() {
                    Some(definition) => format!("{definition}, {expression}"),
                    None => expression,
                });
            }
            if column.descending {
                index.attributes = index.attributes | IndexAttributes::DESCENDING;
            }
            if column.visibility.is_invisible() {
                index.attributes = index.attributes | IndexAttributes::INVISIBLE;
            }
            continue;
        }
        let mut attributes = IndexAttributes::from_parts(!column.non_unique, column.primary);
        if column.expression.is_some() {
            attributes = attributes | IndexAttributes::EXPRESSION;
        }
        if column.descending {
            attributes = attributes | IndexAttributes::DESCENDING;
        }
        if column.visibility.is_invisible() {
            attributes = attributes | IndexAttributes::INVISIBLE;
        }
        indexes.push(Index {
            name: column.name,
            columns: vec![column_name],
            attributes,
            index_type: column
                .index_type
                .to_ascii_lowercase()
                .parse::<IndexType>()
                .unwrap_or_else(|never| match never {}),
            definition: column.expression,
        });
    }
    indexes
}

fn index_column_name(column: &MySqlIndexMetadata) -> String {
    let mut name = column.column_name.clone();
    if column.expression.is_none()
        && let Some(sub_part) = column.sub_part
    {
        name = format!("{name}({sub_part})");
    }
    if column.descending {
        name.push_str(" DESC");
    }
    name
}

fn unique_single_columns_from_metadata(raw: &[MySqlIndexMetadata]) -> HashSet<String> {
    let mut indexes_by_name: HashMap<&str, Vec<&MySqlIndexMetadata>> = HashMap::new();
    for index in raw
        .iter()
        .filter(|index| !index.non_unique && !index.primary)
    {
        indexes_by_name
            .entry(index.name.as_str())
            .or_default()
            .push(index);
    }

    indexes_by_name
        .into_values()
        .filter_map(|parts| {
            let part = parts.first()?;
            (parts.len() == 1 && part.sub_part.is_none() && part.expression.is_none())
                .then(|| part.column_name.clone())
        })
        .collect()
}

#[cfg(all(test, unix))]
mod session_tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    fn fake_metadata_cli(mode: &str) -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join(format!("mysql-{mode}"));
        let transcript = directory.path().join("transcript.log");
        std::fs::write(&transcript, "").unwrap();
        let script = r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
transcript=$(dirname "$0")/transcript.log
printf 'option=%s\nprocess=%s\n' "$option" "$$" >> "$transcript"
if [ -e "$option" ]; then printf 'option-exists=yes\n' >> "$transcript"; else printf 'option-exists=no\n' >> "$transcript"; fi
mode=$(basename "$0" | sed 's/^mysql-//')
trap 'printf "exit=%s\n" "$?" >> "$transcript"' EXIT
if [ "$mode" = "probe-failure" ] || [ "$mode" = "timeout" ]; then
  while [ ! -e "$(dirname "$0")/allow" ]; do sleep 0.001; done
fi
eof=$(printf '\004')
while IFS= read -r line; do
  printf 'query=%s\n' "$line" >> "$transcript"
  [ "$line" = "$eof" ] && exit 0
  [ "$line" = ";" ] && continue
  if printf '%s\n' "$line" | grep -q '__sabiql_probe'; then
    if [ "$mode" = "probe-failure" ]; then
      printf '%s\n' '<resultset><row><field name="wrong">x</field></row></resultset>'
    else
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)'.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
    fi
    continue
  fi
  if printf '%s\n' "$line" | grep -q '__sabiql_inspector_completion'; then
    marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)'.*/\1/")
    if [ "$mode" = "completion-marker-failure" ]; then
      marker=wrong-marker
    fi
    printf '%s\n' '<resultset><row><field name="__sabiql_inspector_completion">'"$marker"'</field></row></resultset>'
    continue
  fi
  case "$line" in
    *"SET SESSION TRANSACTION READ ONLY")
      ;;
    *__sabiql_session_marker*)
      if [ "$mode" = "read-only-failure" ]; then
        printf '%s\n' 'ERROR 1227 (42000): access denied to validate read-only session' >&2
        exit 1
      fi
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_session_marker.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    *TABLES*)
      if [ "$mode" = "empty" ]; then
        printf '%s\n' '<resultset></resultset>'
      elif [ "$mode" = "view" ]; then
        printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items_view</field><field name="TABLE_TYPE">VIEW</field><field name="TABLE_ROWS" xsi:nil="true"/><field name="TABLE_COMMENT">view comment</field><field name="ENGINE" xsi:nil="true"/><field name="ROW_FORMAT" xsi:nil="true"/><field name="TABLE_COLLATION" xsi:nil="true"/><field name="CREATE_OPTIONS"></field></row></resultset>'
      else
        printf '%s\n' '<resultset><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="TABLE_TYPE">BASE TABLE</field><field name="TABLE_ROWS">1</field><field name="TABLE_COMMENT">table comment</field><field name="ENGINE">InnoDB</field><field name="ROW_FORMAT">Dynamic</field><field name="TABLE_COLLATION">utf8mb4_0900_ai_ci</field><field name="CREATE_OPTIONS">partitioned</field></row></resultset>'
      fi
      ;;
    *COLUMNS*)
      if [ "$mode" = "timeout" ]; then
        while :; do sleep 1; done
      elif [ "$mode" = "malformed" ]; then
        printf '%s\n' '<resultset><row><field name="WRONG">x</field></row></resultset>'
      else
        printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="COLUMN_NAME">id</field><field name="COLUMN_TYPE">int</field><field name="IS_NULLABLE">NO</field><field name="COLUMN_DEFAULT" xsi:nil="true"/><field name="EXTRA"></field><field name="COLUMN_COMMENT" xsi:nil="true"/><field name="ORDINAL_POSITION">1</field><field name="PRIMARY_KEY_POSITION">1</field><field name="CHARACTER_SET_NAME" xsi:nil="true"/><field name="COLLATION_NAME" xsi:nil="true"/><field name="GENERATION_EXPRESSION" xsi:nil="true"/></row></resultset>'
      fi
      ;;
    *GROUP\ BY\ s.INDEX_NAME*)
      printf '%s\n' '<resultset></resultset>'
      ;;
    *STATISTICS*)
      printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="INDEX_NAME">PRIMARY</field><field name="NON_UNIQUE">0</field><field name="INDEX_TYPE">BTREE</field><field name="SEQ_IN_INDEX">1</field><field name="COLUMN_NAME" xsi:nil="true"/><field name="SUB_PART" xsi:nil="true"/><field name="EXPRESSION">expr</field><field name="COLLATION" xsi:nil="true"/><field name="IS_VISIBLE">YES</field><field name="IS_PRIMARY">YES</field></row></resultset>'
      ;;
    *FOREIGN*)
      printf '%s\n' '<resultset><row><field name="CONSTRAINT_NAME">fk_items_self</field><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="COLUMN_NAME">id</field><field name="REFERENCED_TABLE_SCHEMA">app</field><field name="REFERENCED_TABLE_NAME">items</field><field name="REFERENCED_COLUMN_NAME">id</field><field name="ORDINAL_POSITION">1</field><field name="UPDATE_RULE">CASCADE</field><field name="DELETE_RULE">CASCADE</field></row></resultset>'
      ;;
    *TRIGGERS*)
      printf '%s\n' '<resultset><row><field name="TRIGGER_NAME">items_audit</field><field name="ACTION_ORDER">1</field><field name="ACTION_TIMING">BEFORE</field><field name="EVENT_MANIPULATION">INSERT</field><field name="ACTION_STATEMENT">SET NEW.id = NEW.id</field><field name="DEFINER">app@localhost</field><field name="SQL_MODE">STRICT_TRANS_TABLES</field><field name="CHARACTER_SET_CLIENT">utf8mb4</field><field name="COLLATION_CONNECTION">utf8mb4_0900_ai_ci</field><field name="DATABASE_COLLATION">utf8mb4_0900_ai_ci</field><field name="CREATED">2026-08-21 10:20:30.00</field></row></resultset>'
      ;;
    *SHOW\ CREATE\ VIEW*)
      if [ "$mode" = "view" ]; then
        printf '%s\n' '<resultset><row><field name="View">items_view</field><field name="Create View">CREATE VIEW items_view AS SELECT 1</field></row></resultset>'
      fi
      ;;
    *SHOW\ CREATE\ TABLE*)
      printf '%s\n' '<resultset><row><field name="Table">items</field><field name="Create Table">CREATE TABLE items (id int PRIMARY KEY)</field></row></resultset>'
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
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!(
            "fake mysql process is alive or did not start: {}",
            std::fs::read_to_string(transcript).unwrap()
        );
    }

    fn assert_option_file_removed(transcript: &std::path::Path) {
        let transcript_text = std::fs::read_to_string(transcript).unwrap();
        assert!(transcript_text.contains("option-exists=yes"));
        let option = transcript_text
            .lines()
            .find_map(|line| line.strip_prefix("option="))
            .unwrap();
        assert!(
            !std::path::Path::new(option).exists(),
            "option file remains"
        );
    }

    #[tokio::test]
    async fn inspector_detail_orchestration_uses_one_process_for_table_and_view() {
        for (mode, schema, table) in [("table", "app", "items"), ("view", "app", "items_view")] {
            let (_directory, program, transcript) = fake_metadata_cli(mode);
            let detail = fetch_table_detail_in_session_with_program(
                "mysql://user:password@localhost:3306/app",
                schema,
                table,
                OsStr::new(&program),
                Duration::from_secs(5),
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "fake metadata CLI failed: {error:?}\n{}",
                    std::fs::read_to_string(&transcript).unwrap()
                )
            });

            assert_eq!(detail.name, table);
            assert_eq!(detail.columns.len(), 1);
            assert!(detail.source_ddl().is_some());
            if mode == "table" {
                assert_eq!(detail.storage_attributes.engine.as_deref(), Some("InnoDB"));
                assert_eq!(
                    detail.storage_attributes.row_format.as_deref(),
                    Some("Dynamic")
                );
                assert_eq!(
                    detail.storage_attributes.table_collation.as_deref(),
                    Some("utf8mb4_0900_ai_ci")
                );
                assert_eq!(
                    detail.storage_attributes.create_options.as_deref(),
                    Some("partitioned")
                );
            } else {
                assert!(detail.storage_attributes.engine.is_none());
                assert!(detail.storage_attributes.row_format.is_none());
                assert!(detail.storage_attributes.table_collation.is_none());
                assert!(detail.storage_attributes.create_options.is_none());
            }
            let transcript_text = std::fs::read_to_string(&transcript).unwrap();
            assert_eq!(
                transcript_text
                    .lines()
                    .filter(|line| line.starts_with("process="))
                    .count(),
                1
            );
            let labels = if mode == "view" {
                [
                    "SET SESSION autocommit=1, completion_type=NO_CHAIN",
                    "SET SESSION TRANSACTION READ ONLY",
                    "__sabiql_session_marker",
                    "__sabiql_sql_mode",
                    "__sabiql_probe",
                    "INFORMATION_SCHEMA.TABLES",
                    "INFORMATION_SCHEMA.COLUMNS",
                    "INFORMATION_SCHEMA.STATISTICS",
                    "REFERENTIAL_CONSTRAINTS",
                    "INFORMATION_SCHEMA.TRIGGERS",
                    "SHOW CREATE VIEW",
                    "__sabiql_inspector_completion",
                ]
            } else {
                [
                    "SET SESSION autocommit=1, completion_type=NO_CHAIN",
                    "SET SESSION TRANSACTION READ ONLY",
                    "__sabiql_session_marker",
                    "__sabiql_sql_mode",
                    "__sabiql_probe",
                    "INFORMATION_SCHEMA.TABLES",
                    "INFORMATION_SCHEMA.COLUMNS",
                    "INFORMATION_SCHEMA.STATISTICS",
                    "REFERENTIAL_CONSTRAINTS",
                    "INFORMATION_SCHEMA.TRIGGERS",
                    "SHOW CREATE TABLE",
                    "__sabiql_inspector_completion",
                ]
            };
            let positions = labels
                .into_iter()
                .map(|label| transcript_text.find(label).unwrap())
                .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
            assert_process_stopped(&transcript);
            assert_option_file_removed(&transcript);
        }
    }

    #[tokio::test]
    async fn inspector_detail_rejects_a_mismatched_completion_marker() {
        let (_directory, program, transcript) = fake_metadata_cli("completion-marker-failure");
        let error = fetch_table_detail_in_session_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();

        assert!(
            matches!(error, DbOperationError::QueryFailed(details) if details.contains("completion marker"))
        );
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);
    }

    #[tokio::test]
    async fn completion_detail_prefetch_uses_one_process_and_four_metadata_queries() {
        let (_directory, program, transcript) = fake_metadata_cli("table");
        let detail = fetch_table_columns_and_fks_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "fake completion metadata CLI failed: {error:?}\n{}",
                std::fs::read_to_string(&transcript).unwrap()
            )
        });

        assert_eq!(detail.name, "items");
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
                    line.starts_with("query=")
                        && (line.contains("INFORMATION_SCHEMA")
                            || line.contains("REFERENTIAL_CONSTRAINTS"))
                })
                .count(),
            4
        );
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);
    }

    #[tokio::test]
    async fn inspector_detail_read_only_setup_failure_never_sends_metadata_sql() {
        let (_directory, program, transcript) = fake_metadata_cli("read-only-failure");
        let result = fetch_table_detail_in_session_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await;

        let transcript_text = std::fs::read_to_string(&transcript).unwrap();
        assert!(result.is_err(), "result={result:?}\n{transcript_text}");
        assert!(transcript_text.contains("SET SESSION TRANSACTION READ ONLY"));
        assert!(!transcript_text.contains("INFORMATION_SCHEMA.TABLES"));
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);
    }

    #[tokio::test]
    async fn inspector_detail_orchestration_rejects_empty_and_malformed_shapes_without_partial_table()
     {
        let (_directory, program, transcript) = fake_metadata_cli("empty");
        let error = fetch_table_detail_in_session_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, DbOperationError::ObjectMissing(_)));
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);

        let (_directory, program, transcript) = fake_metadata_cli("malformed");
        let error = fetch_table_detail_in_session_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, DbOperationError::MetadataParseFailed(_)));
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);
    }

    #[tokio::test]
    async fn inspector_detail_orchestration_cleans_up_after_probe_failure_and_timeout() {
        for (mode, timeout) in [
            ("probe-failure", Duration::from_secs(5)),
            ("timeout", Duration::from_secs(5)),
        ] {
            let (directory, program, transcript) = fake_metadata_cli(mode);
            let task = tokio::spawn(async move {
                fetch_table_detail_in_session_with_program(
                    "mysql://user:password@localhost:3306/app",
                    "app",
                    "items",
                    OsStr::new(&program),
                    timeout,
                )
                .await
            });
            for _ in 0..1_000 {
                if std::fs::read_to_string(&transcript)
                    .unwrap()
                    .lines()
                    .any(|line| line.starts_with("process="))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            std::fs::write(directory.path().join("allow"), "").unwrap();
            let error = task.await.unwrap().unwrap_err();
            if mode == "timeout" {
                assert!(matches!(error, DbOperationError::Timeout(_)));
            } else {
                assert!(matches!(error, DbOperationError::QueryFailed(_)));
            }
            assert_process_stopped(&transcript);
            assert_option_file_removed(&transcript);
        }
    }

    #[tokio::test]
    async fn inspector_detail_orchestration_cleans_up_after_cancellation() {
        let (_directory, program, transcript) = fake_metadata_cli("timeout");
        let task = tokio::spawn(async move {
            fetch_table_detail_in_session_with_program(
                "mysql://user:password@localhost:3306/app",
                "app",
                "items",
                OsStr::new(&program),
                Duration::from_secs(5),
            )
            .await
        });
        for _ in 0..1_000 {
            if std::fs::read_to_string(&transcript)
                .unwrap()
                .lines()
                .any(|line| line.starts_with("process="))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());

        for _ in 0..100 {
            let process_stopped = std::fs::read_to_string(&transcript)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("process=")?.parse::<libc::pid_t>().ok())
                .is_some_and(|pid| unsafe { libc::kill(pid, 0) } == -1);
            if process_stopped {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_process_stopped(&transcript);
        assert_option_file_removed(&transcript);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QueryValue;

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    #[test]
    fn trigger_metadata_preserves_action_order_context_and_definition() {
        let result = result(
            &[
                "TRIGGER_NAME",
                "ACTION_ORDER",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
                "SQL_MODE",
                "CHARACTER_SET_CLIENT",
                "COLLATION_CONNECTION",
                "DATABASE_COLLATION",
                "CREATED",
            ],
            vec![
                vec![
                    QueryValue::Text("z_add".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("UPDATE".to_string()),
                    QueryValue::Text("SET @seen = 1".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                    QueryValue::Text("STRICT_TRANS_TABLES".to_string()),
                    QueryValue::Text("utf8mb4".to_string()),
                    QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
                    QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
                    QueryValue::Text("2026-08-21 10:20:30.00".to_string()),
                ],
                vec![
                    QueryValue::Text("a_double".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("UPDATE".to_string()),
                    QueryValue::Text("SET @seen = 2".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                    QueryValue::Text("STRICT_TRANS_TABLES".to_string()),
                    QueryValue::Text("utf8mb4".to_string()),
                    QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
                    QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
                    QueryValue::Text("2026-08-21 10:20:31.00".to_string()),
                ],
            ],
        );

        let triggers = parse_trigger_metadata(&result).unwrap();

        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[0].name, "z_add");
        assert_eq!(triggers[0].action_order, Some(1));
        assert_eq!(triggers[0].events, [TriggerEvent::Update]);
        assert_eq!(triggers[0].definition, "SET @seen = 1");
        assert_eq!(triggers[0].security_context.as_deref(), Some("sabiql@%"));
        assert_eq!(
            triggers[0].creation_context,
            Some(TriggerCreationContext {
                sql_mode: Some("STRICT_TRANS_TABLES".to_string()),
                character_set_client: Some("utf8mb4".to_string()),
                collation_connection: Some("utf8mb4_0900_ai_ci".to_string()),
                database_collation: Some("utf8mb4_0900_ai_ci".to_string()),
                created: Some("2026-08-21 10:20:30.00".to_string()),
            })
        );
        assert_eq!(triggers[1].name, "a_double");
        assert_eq!(triggers[1].action_order, Some(2));
        assert_eq!(triggers[1].events, [TriggerEvent::Update]);
    }

    #[test]
    fn empty_trigger_result_returns_no_triggers() {
        let result = result(TRIGGER_RESULT_COLUMNS, Vec::new());

        assert!(parse_trigger_metadata(&result).unwrap().is_empty());
    }

    #[test]
    fn show_create_parser_preserves_view_ddl_and_extra_server_columns() {
        let ddl = "CREATE ALGORITHM=UNDEFINED VIEW `v` AS select 1";
        let result = result(
            &[
                "View",
                "Create View",
                "character_set_client",
                "collation_connection",
            ],
            vec![vec![
                QueryValue::Text("v".to_string()),
                QueryValue::Text(ddl.to_string()),
                QueryValue::Text("utf8mb4".to_string()),
                QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
            ]],
        );

        assert_eq!(parse_source_ddl(&result, TableKind::View).unwrap(), ddl);
    }

    #[test]
    fn groups_indexes_by_name_and_orders_columns_by_sequence() {
        let result = result(
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "SUB_PART",
                "EXPRESSION",
                "COLLATION",
                "IS_VISIBLE",
                "IS_PRIMARY",
            ],
            vec![
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("search_index".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("FULLTEXT".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("body".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
            ],
        );

        let indexes = indexes_from_metadata(parse_index_metadata(&result).unwrap());

        assert_eq!(indexes[0].name, "PRIMARY");
        assert_eq!(indexes[0].columns, ["first_key", "second_key"]);
        assert!(indexes[0].is_primary());
        assert_eq!(
            indexes[1].index_type,
            IndexType::Other("fulltext".to_string())
        );
        assert!(!indexes[1].is_unique());
    }

    #[test]
    fn maps_prefix_descending_and_invisible_key_parts() {
        let result = result(
            INDEX_RESULT_COLUMNS,
            vec![
                vec![
                    QueryValue::Text("idx_email_created".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("email".to_string()),
                    QueryValue::Text("8".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("D".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_email_created".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("created_at".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("A".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
            ],
        );

        let index = indexes_from_metadata(parse_index_metadata(&result).unwrap())
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(index.columns, ["email(8) DESC", "created_at"]);
        assert!(index.has_descending_key());
        assert!(index.is_invisible());
    }

    #[test]
    fn single_column_unique_indexes_exclude_prefix_and_non_single_indexes() {
        let result = result(
            INDEX_RESULT_COLUMNS,
            vec![
                vec![
                    QueryValue::Text("uq_email".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("email".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_email_prefix".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("email".to_string()),
                    QueryValue::Text("8".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_prefix_pair".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("email".to_string()),
                    QueryValue::Text("8".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_prefix_pair".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_pair".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_pair".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_email".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("email".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("uq_expression".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("lower(`email`)".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
            ],
        );

        let raw = parse_index_metadata(&result).unwrap();

        assert_eq!(
            unique_single_columns_from_metadata(&raw),
            HashSet::from(["email".to_string()])
        );
        let indexes = indexes_from_metadata(raw);
        assert!(
            indexes
                .iter()
                .find(|index| index.name == "uq_email_prefix")
                .is_some_and(Index::is_unique)
        );
    }

    #[test]
    fn parses_functional_and_mixed_indexes_in_key_part_order() {
        let result = result(
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "SUB_PART",
                "EXPRESSION",
                "COLLATION",
                "IS_VISIBLE",
                "IS_PRIMARY",
            ],
            vec![
                vec![
                    QueryValue::Text("idx_mixed".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("sort_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_functional".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("lower(`payload`->>'$.code')".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_mixed".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                    QueryValue::Null,
                    QueryValue::Text("lower(`payload`->>'$.code')".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
            ],
        );

        let indexes = indexes_from_metadata(parse_index_metadata(&result).unwrap());
        let functional = &indexes[0];
        assert_eq!(functional.name, "idx_functional");
        assert_eq!(
            functional.columns,
            ["lower(`payload`->>'$.code')".to_string()]
        );
        assert!(functional.has_expression());
        assert_eq!(
            functional.definition.as_deref(),
            Some("lower(`payload`->>'$.code')")
        );

        let mixed = &indexes[1];
        assert_eq!(mixed.name, "idx_mixed");
        assert_eq!(
            mixed.columns,
            [
                "lower(`payload`->>'$.code')".to_string(),
                "sort_key".to_string()
            ]
        );
        assert!(mixed.has_expression());
        assert_eq!(
            mixed.definition.as_deref(),
            Some("lower(`payload`->>'$.code')")
        );
    }

    #[test]
    fn rejects_index_key_part_without_column_or_expression() {
        let result = result(
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "SUB_PART",
                "EXPRESSION",
                "COLLATION",
                "IS_VISIBLE",
                "IS_PRIMARY",
            ],
            vec![vec![
                QueryValue::Text("idx_invalid".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Text("BTREE".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Text("YES".to_string()),
                QueryValue::Text("NO".to_string()),
            ]],
        );

        let error = parse_index_metadata(&result).unwrap_err();
        assert!(matches!(
            error,
            DbOperationError::MetadataParseFailed(message)
                if message.contains("neither COLUMN_NAME nor EXPRESSION")
        ));
    }
}
