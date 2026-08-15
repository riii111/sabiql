use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    ForeignKey, Index, IndexAttributes, IndexType, Table, TableKind, TableKindInfo, Trigger,
    TriggerEvent, TriggerTiming,
};

use super::super::sql::{quote_identifier, quote_string};
use super::super::{
    cli::{MYSQL_QUERY_TIMEOUT, MysqlMetadataSession, MysqlResultSet},
    dsn::parse_and_validate_mysql_dsn,
    option_file::MySqlOptionFile,
};
use super::catalog::{
    COLUMN_METADATA_RESULT_COLUMNS, FOREIGN_KEY_RESULT_COLUMNS, MysqlColumnMetadata,
    MysqlTableMetadata, TABLES_RESULT_COLUMNS, column_from_metadata, columns_query,
    execute_metadata_queries_in_session, expect_columns, find_table, foreign_keys_from_metadata,
    foreign_keys_query, metadata_shape_error, metadata_snapshot_from_result, optional_text,
    parse_boolean_flag, parse_columns_for_table, parse_foreign_key_metadata, parse_positive_i32,
    primary_key_names, required_text, selected_database, table_query,
    validate_selected_schema_name,
};

#[derive(Debug, Clone)]
struct MysqlIndexMetadata {
    name: String,
    non_unique: bool,
    index_type: String,
    ordinal_position: i32,
    column_name: String,
    expression: Option<String>,
    primary: bool,
}

#[derive(Debug, Clone)]
struct MysqlTriggerMetadata {
    name: String,
    timing: TriggerTiming,
    event: TriggerEvent,
    definition: String,
    security_context: Option<String>,
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
    let database = selected_database(dsn)?;
    validate_selected_schema_name(&database, schema)?;
    let table_query = table_query(schema, table);
    let columns_query = columns_query(schema, table);
    let foreign_keys_query = foreign_keys_query(schema, table);
    let results = execute_metadata_queries_in_session(
        dsn,
        &[
            (table_query.as_str(), TABLES_RESULT_COLUMNS),
            (columns_query.as_str(), COLUMN_METADATA_RESULT_COLUMNS),
            (foreign_keys_query.as_str(), FOREIGN_KEY_RESULT_COLUMNS),
        ],
    )
    .await?;
    let snapshot = metadata_snapshot_from_result(&database, Some(schema), &results[0])?;
    let table_metadata = find_table(schema, table, &snapshot.tables)?;
    let columns = parse_columns_for_table(&results[1], schema, table)?;
    let foreign_keys =
        foreign_keys_from_metadata(parse_foreign_key_metadata(&results[2])?, &database)?;
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
    let database = target.database.as_deref().ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "MySQL metadata requires a selected database".to_string(),
        )
    })?;
    validate_selected_schema_name(database, schema)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut session = MysqlMetadataSession::spawn_with_program(program, &option_file.path)?;
    let result = tokio::time::timeout(
        timeout,
        fetch_table_detail_with_session(&mut session, database, schema, table),
    )
    .await;
    let result = match result {
        Ok(Ok(table)) => Ok(table),
        Ok(Err(error)) => {
            session.cleanup().await;
            Err(error)
        }
        Err(_) => {
            session.cleanup().await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    };
    drop(option_file);
    result
}

async fn fetch_table_detail_with_session(
    session: &mut MysqlMetadataSession,
    database: &str,
    schema: &str,
    table: &str,
) -> Result<Table, DbOperationError> {
    session.probe().await?;
    session.prepare_read_only().await?;
    let tables_result = session
        .execute_with_expected_columns(&table_query(schema, table), TABLES_RESULT_COLUMNS)
        .await?;
    let snapshot = metadata_snapshot_from_result(database, Some(schema), &tables_result)?;
    let table_metadata = find_table(schema, table, &snapshot.tables)?;

    let columns = parse_columns_for_table(
        &session
            .execute_with_expected_columns(
                &columns_query(schema, table),
                COLUMN_METADATA_RESULT_COLUMNS,
            )
            .await?,
        schema,
        table,
    )?;
    let indexes = indexes_from_metadata(parse_index_metadata(
        &session
            .execute_with_expected_columns(&indexes_query(table), INDEX_RESULT_COLUMNS)
            .await?,
    )?);
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
    )?;
    let triggers = triggers_from_metadata(parse_trigger_metadata(
        &session
            .execute_with_expected_columns(&triggers_query(table), TRIGGER_RESULT_COLUMNS)
            .await?,
    )?)?;
    let source_ddl = parse_source_ddl(
        &session
            .execute_with_expected_columns(
                &show_create_query(table, table_metadata.kind),
                show_create_result_columns(table_metadata.kind),
            )
            .await?,
        table_metadata.kind,
    )?;
    session.finish().await?;

    let primary_key = primary_key_names(&columns);
    Ok(Table {
        schema: table_metadata.schema,
        name: table_metadata.name,
        owner: None,
        columns: columns.iter().map(column_from_metadata).collect(),
        primary_key: (!primary_key.is_empty()).then_some(primary_key),
        foreign_keys,
        indexes,
        rls: None,
        triggers,
        row_count_estimate: table_metadata.row_count_estimate,
        comment: table_metadata.comment,
        source_ddl: Some(source_ddl),
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            is_strict: false,
            without_rowid: false,
            virtual_module: None,
        },
    })
}

fn table_from_columns_and_foreign_keys(
    table_metadata: MysqlTableMetadata,
    columns: Vec<MysqlColumnMetadata>,
    foreign_keys: Vec<ForeignKey>,
) -> Table {
    let primary_key = primary_key_names(&columns);
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
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            is_strict: false,
            without_rowid: false,
            virtual_module: None,
        },
    }
}

const INDEX_RESULT_COLUMNS: &[&str] = &[
    "INDEX_NAME",
    "NON_UNIQUE",
    "INDEX_TYPE",
    "SEQ_IN_INDEX",
    "COLUMN_NAME",
    "EXPRESSION",
    "IS_PRIMARY",
];
const TRIGGER_RESULT_COLUMNS: &[&str] = &[
    "TRIGGER_NAME",
    "ACTION_TIMING",
    "EVENT_MANIPULATION",
    "ACTION_STATEMENT",
    "DEFINER",
];
const TABLE_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["Table", "Create Table"];
const VIEW_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["View", "Create View"];

fn show_create_result_columns(kind: TableKind) -> &'static [&'static str] {
    if kind == TableKind::View {
        VIEW_SHOW_CREATE_RESULT_COLUMNS
    } else {
        TABLE_SHOW_CREATE_RESULT_COLUMNS
    }
}

fn indexes_query(table: &str) -> String {
    format!(
        "SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, s.COLUMN_NAME, s.EXPRESSION, CASE WHEN tc.CONSTRAINT_TYPE = 'PRIMARY KEY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY FROM INFORMATION_SCHEMA.STATISTICS AS s LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_NAME = s.TABLE_NAME AND tc.CONSTRAINT_NAME = s.INDEX_NAME WHERE s.TABLE_SCHEMA = DATABASE() AND s.TABLE_NAME = {} ORDER BY INDEX_NAME, SEQ_IN_INDEX",
        quote_string(table),
    )
}

fn triggers_query(table: &str) -> String {
    format!(
        "SELECT TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION, ACTION_STATEMENT, DEFINER FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND EVENT_OBJECT_SCHEMA = DATABASE() AND EVENT_OBJECT_TABLE = {} ORDER BY TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION",
        quote_string(table),
    )
}

fn show_create_query(table: &str, kind: TableKind) -> String {
    let object_type = if kind == TableKind::View {
        "VIEW"
    } else {
        "TABLE"
    };
    format!("SHOW CREATE {object_type} {}", quote_identifier(table))
}

fn parse_trigger_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlTriggerMetadata>, DbOperationError> {
    expect_columns(result, TRIGGER_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 5 {
                return Err(metadata_shape_error("TRIGGERS row"));
            }
            let timing = required_text(&row[1], "ACTION_TIMING")?
                .parse::<TriggerTiming>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            let event = required_text(&row[2], "EVENT_MANIPULATION")?
                .parse::<TriggerEvent>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            Ok(MysqlTriggerMetadata {
                name: required_text(&row[0], "TRIGGER_NAME")?.to_string(),
                timing,
                event,
                definition: required_text(&row[3], "ACTION_STATEMENT")?.to_string(),
                security_context: optional_text(&row[4], "DEFINER")?.map(str::to_string),
            })
        })
        .collect()
}

fn triggers_from_metadata(
    raw: Vec<MysqlTriggerMetadata>,
) -> Result<Vec<Trigger>, DbOperationError> {
    let mut triggers = Vec::new();
    for metadata in raw {
        if let Some(trigger) = triggers
            .iter_mut()
            .find(|trigger: &&mut Trigger| trigger.name == metadata.name)
        {
            if trigger.timing != metadata.timing
                || trigger.definition != metadata.definition
                || trigger.security_context != metadata.security_context
            {
                return Err(metadata_shape_error("TRIGGERS definition"));
            }
            trigger.events.push(metadata.event);
        } else {
            triggers.push(Trigger {
                name: metadata.name,
                timing: metadata.timing,
                events: vec![metadata.event],
                definition: metadata.definition,
                security_context: metadata.security_context,
            });
        }
    }
    Ok(triggers)
}

fn parse_source_ddl(result: &MysqlResultSet, kind: TableKind) -> Result<String, DbOperationError> {
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
    result: &MysqlResultSet,
) -> Result<Vec<MysqlIndexMetadata>, DbOperationError> {
    expect_columns(result, INDEX_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 7 {
                return Err(metadata_shape_error("STATISTICS row"));
            }
            let column_name = optional_text(&row[4], "COLUMN_NAME")?;
            let expression = optional_text(&row[5], "EXPRESSION")?;
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
            Ok(MysqlIndexMetadata {
                name: required_text(&row[0], "INDEX_NAME")?.to_string(),
                non_unique: parse_boolean_flag(&row[1], "NON_UNIQUE")?,
                index_type: required_text(&row[2], "INDEX_TYPE")?.to_string(),
                ordinal_position: parse_positive_i32(&row[3], "SEQ_IN_INDEX")?,
                column_name,
                expression,
                primary: parse_boolean_flag(&row[6], "IS_PRIMARY")?,
            })
        })
        .collect()
}

fn indexes_from_metadata(mut raw: Vec<MysqlIndexMetadata>) -> Vec<Index> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut indexes = Vec::new();
    for column in raw {
        if let Some(index) = indexes
            .iter_mut()
            .find(|index: &&mut Index| index.name == column.name)
        {
            index.columns.push(column.column_name);
            if let Some(expression) = column.expression {
                index.attributes = index.attributes | IndexAttributes::EXPRESSION;
                index.definition = Some(match index.definition.take() {
                    Some(definition) => format!("{definition}, {expression}"),
                    None => expression,
                });
            }
            continue;
        }
        let mut attributes = IndexAttributes::from_parts(!column.non_unique, column.primary);
        if column.expression.is_some() {
            attributes = attributes | IndexAttributes::EXPRESSION;
        }
        indexes.push(Index {
            name: column.name,
            columns: vec![column.column_name],
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
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
    fi
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
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      ;;
    *TABLES*)
      if [ "$mode" = "empty" ]; then
        printf '%s\n' '<resultset></resultset>'
      elif [ "$mode" = "view" ]; then
        printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items_view</field><field name="TABLE_TYPE">VIEW</field><field name="TABLE_ROWS" xsi:nil="true"/><field name="TABLE_COMMENT">view comment</field></row></resultset>'
      else
        printf '%s\n' '<resultset><row><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="TABLE_TYPE">BASE TABLE</field><field name="TABLE_ROWS">1</field><field name="TABLE_COMMENT">table comment</field></row></resultset>'
      fi
      ;;
    *COLUMNS*)
      if [ "$mode" = "timeout" ]; then
        while :; do sleep 1; done
      elif [ "$mode" = "malformed" ]; then
        printf '%s\n' '<resultset><row><field name="WRONG">x</field></row></resultset>'
      else
        printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="COLUMN_NAME">id</field><field name="COLUMN_TYPE">int</field><field name="IS_NULLABLE">NO</field><field name="COLUMN_DEFAULT" xsi:nil="true"/><field name="EXTRA"></field><field name="COLUMN_COMMENT" xsi:nil="true"/><field name="ORDINAL_POSITION">1</field><field name="PRIMARY_KEY_POSITION">1</field></row></resultset>'
      fi
      ;;
    *STATISTICS*)
      printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="INDEX_NAME">PRIMARY</field><field name="NON_UNIQUE">0</field><field name="INDEX_TYPE">BTREE</field><field name="SEQ_IN_INDEX">1</field><field name="COLUMN_NAME" xsi:nil="true"/><field name="EXPRESSION">expr</field><field name="IS_PRIMARY">YES</field></row></resultset>'
      ;;
    *FOREIGN*)
      printf '%s\n' '<resultset><row><field name="CONSTRAINT_NAME">fk_items_self</field><field name="TABLE_SCHEMA">app</field><field name="TABLE_NAME">items</field><field name="COLUMN_NAME">id</field><field name="REFERENCED_TABLE_SCHEMA">app</field><field name="REFERENCED_TABLE_NAME">items</field><field name="REFERENCED_COLUMN_NAME">id</field><field name="ORDINAL_POSITION">1</field><field name="UPDATE_RULE">CASCADE</field><field name="DELETE_RULE">CASCADE</field></row></resultset>'
      ;;
    *TRIGGERS*)
      printf '%s\n' '<resultset><row><field name="TRIGGER_NAME">items_audit</field><field name="ACTION_TIMING">BEFORE</field><field name="EVENT_MANIPULATION">INSERT</field><field name="ACTION_STATEMENT">SET NEW.id = NEW.id</field><field name="DEFINER">app@localhost</field></row></resultset>'
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
                    "__sabiql_probe",
                    "SET SESSION TRANSACTION READ ONLY",
                    "__sabiql_session_marker",
                    "INFORMATION_SCHEMA.TABLES",
                    "INFORMATION_SCHEMA.COLUMNS",
                    "INFORMATION_SCHEMA.STATISTICS",
                    "REFERENTIAL_CONSTRAINTS",
                    "INFORMATION_SCHEMA.TRIGGERS",
                    "SHOW CREATE VIEW",
                ]
            } else {
                [
                    "__sabiql_probe",
                    "SET SESSION TRANSACTION READ ONLY",
                    "__sabiql_session_marker",
                    "INFORMATION_SCHEMA.TABLES",
                    "INFORMATION_SCHEMA.COLUMNS",
                    "INFORMATION_SCHEMA.STATISTICS",
                    "REFERENTIAL_CONSTRAINTS",
                    "INFORMATION_SCHEMA.TRIGGERS",
                    "SHOW CREATE TABLE",
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
    async fn inspector_detail_sends_foreign_key_query_without_sentinel() {
        let (_directory, program, transcript) = fake_metadata_cli("table");
        fetch_table_detail_in_session_with_program(
            "mysql://user:password@localhost:3306/app",
            "app",
            "items",
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        let transcript_text = std::fs::read_to_string(&transcript).unwrap();
        let foreign_key_query = transcript_text
            .lines()
            .find(|line| line.contains("REFERENTIAL_CONSTRAINTS"))
            .expect("foreign key metadata query");
        assert!(!foreign_key_query.contains("UNION ALL SELECT NULL"));
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

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MysqlResultSet {
        MysqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    #[test]
    fn trigger_metadata_preserves_action_definer_and_event_order() {
        let result = result(
            &[
                "TRIGGER_NAME",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
            ],
            vec![
                vec![
                    QueryValue::Text("audit_changes".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("INSERT".to_string()),
                    QueryValue::Text("BEGIN\n  SET @seen = 1;\nEND".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                ],
                vec![
                    QueryValue::Text("audit_changes".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("UPDATE".to_string()),
                    QueryValue::Text("BEGIN\n  SET @seen = 1;\nEND".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                ],
            ],
        );

        let triggers = triggers_from_metadata(parse_trigger_metadata(&result).unwrap()).unwrap();

        assert_eq!(triggers.len(), 1);
        assert_eq!(
            triggers[0].events,
            [TriggerEvent::Insert, TriggerEvent::Update]
        );
        assert_eq!(triggers[0].definition, "BEGIN\n  SET @seen = 1;\nEND");
        assert_eq!(triggers[0].security_context.as_deref(), Some("sabiql@%"));
    }

    #[test]
    fn empty_trigger_result_returns_no_triggers() {
        let result = result(TRIGGER_RESULT_COLUMNS, Vec::new());

        assert!(parse_trigger_metadata(&result).unwrap().is_empty());
    }

    #[test]
    fn show_create_query_uses_object_kind_and_identifier_quoting() {
        assert_eq!(
            show_create_query("table`name", TableKind::Table),
            "SHOW CREATE TABLE `table``name`"
        );
        assert_eq!(
            show_create_query("view_name", TableKind::View),
            "SHOW CREATE VIEW `view_name`"
        );
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
                "EXPRESSION",
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
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("search_index".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("FULLTEXT".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("body".to_string()),
                    QueryValue::Null,
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
    fn parses_functional_and_mixed_indexes_in_key_part_order() {
        let result = result(
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "EXPRESSION",
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
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_functional".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("lower(`payload`->>'$.code')".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
                vec![
                    QueryValue::Text("idx_mixed".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("lower(`payload`->>'$.code')".to_string()),
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
                "EXPRESSION",
                "IS_PRIMARY",
            ],
            vec![vec![
                QueryValue::Text("idx_invalid".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Text("BTREE".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Null,
                QueryValue::Null,
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
