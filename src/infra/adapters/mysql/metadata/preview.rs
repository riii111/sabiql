use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{Column, QueryValue};

use super::super::{
    cli::{
        MYSQL_QUERY_TIMEOUT, MySqlMetadataSession, MySqlResultSet,
        validate_mysql_multi_query_with_lower_case_table_names,
    },
    dsn::parse_and_validate_mysql_dsn,
    option_file::MySqlOptionFile,
    sql::{
        PREVIEW_COLUMN_METADATA_RESULT_COLUMNS, build_preview_query, preview_columns_query,
        preview_identity_alias,
    },
};
use super::catalog::{
    MySqlColumnMetadata, column_from_metadata, parse_preview_columns_for_table, primary_key_names,
    selected_database, validate_selected_schema_name,
};

#[derive(Debug, Clone)]
pub(in crate::adapters::mysql) struct PreviewMetadata {
    pub(in crate::adapters::mysql) visible_columns: Vec<Column>,
    pub(in crate::adapters::mysql) order_columns: Vec<String>,
    pub(in crate::adapters::mysql) identity_columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::adapters::mysql) struct ConvertedPreviewValues {
    pub(in crate::adapters::mysql) visible: Vec<Vec<QueryValue>>,
    pub(in crate::adapters::mysql) identity: Option<Vec<Vec<QueryValue>>>,
}

pub(in crate::adapters::mysql) struct PreviewExecution {
    pub(in crate::adapters::mysql) metadata: PreviewMetadata,
    pub(in crate::adapters::mysql) result_set: MySqlResultSet,
    pub(in crate::adapters::mysql) display_query: String,
}

pub(in crate::adapters::mysql) async fn execute_preview(
    dsn: &str,
    schema: &str,
    table: &str,
    limit: usize,
    offset: usize,
) -> Result<PreviewExecution, DbOperationError> {
    execute_preview_with_program(
        dsn,
        schema,
        table,
        limit,
        offset,
        OsStr::new("mysql"),
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

async fn execute_preview_with_program(
    dsn: &str,
    schema: &str,
    table: &str,
    limit: usize,
    offset: usize,
    program: &OsStr,
    timeout: Duration,
) -> Result<PreviewExecution, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let database = selected_database(&target)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut session = MySqlMetadataSession::spawn_with_program(program, option_file)?;
    let result = tokio::time::timeout(
        timeout,
        execute_preview_with_session(&mut session, database, schema, table, limit, offset),
    )
    .await;
    session.resolve_timed_result(result).await
}

async fn execute_preview_with_session(
    session: &mut MySqlMetadataSession,
    database: &str,
    schema: &str,
    table: &str,
    limit: usize,
    offset: usize,
) -> Result<PreviewExecution, DbOperationError> {
    let lower_case_table_names = session.prepare_read_only_and_probe().await?;
    validate_selected_schema_name(database, schema, lower_case_table_names)?;

    let column_result = session
        .execute_with_expected_columns(
            &preview_columns_query(schema, table),
            PREVIEW_COLUMN_METADATA_RESULT_COLUMNS,
        )
        .await?;
    let column_metadata = parse_preview_columns_for_table(&column_result, schema, table)?;
    let metadata = preview_metadata_from_columns(&column_metadata, schema, table)?;
    let query = build_preview_query(
        schema,
        table,
        &metadata.order_columns,
        &metadata.visible_columns,
        &metadata.identity_columns,
        limit,
        offset,
    );
    validate_mysql_multi_query_with_lower_case_table_names(
        &query,
        Some(database),
        AccessMode::ReadOnly,
        lower_case_table_names,
    )?;
    let expected_columns =
        preview_result_columns(&metadata.visible_columns, &metadata.identity_columns);
    let result_set = session
        .execute_with_expected_columns(
            &query,
            &expected_columns
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .await?;
    let display_query = build_preview_query(
        schema,
        table,
        &metadata.order_columns,
        &metadata.visible_columns,
        &[],
        limit,
        offset,
    );
    session.finish_preview().await?;

    Ok(PreviewExecution {
        metadata,
        result_set,
        display_query,
    })
}

fn preview_metadata_from_columns(
    column_metadata: &[MySqlColumnMetadata],
    schema: &str,
    table: &str,
) -> Result<PreviewMetadata, DbOperationError> {
    let columns = column_metadata
        .iter()
        .map(column_from_metadata)
        .collect::<Vec<_>>();
    let visible_columns = columns
        .iter()
        .filter(|column| !column.is_hidden())
        .cloned()
        .collect::<Vec<_>>();
    if visible_columns.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL object has no visible columns: {schema}.{table}"
        )));
    }
    let primary_key_names = primary_key_names(column_metadata);
    let identity_columns = if primary_key_names.iter().any(|name| {
        columns
            .iter()
            .find(|column| &column.name == name)
            .is_some_and(Column::is_hidden)
    }) {
        primary_key_names
            .iter()
            .filter_map(|name| columns.iter().find(|column| &column.name == name))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let order_columns = primary_key_names;
    Ok(PreviewMetadata {
        visible_columns,
        order_columns,
        identity_columns,
    })
}

pub(in crate::adapters::mysql) fn convert_preview_values_with_binary_charset(
    result: &MySqlResultSet,
    columns: &[Column],
    identity_columns: &[Column],
) -> Result<ConvertedPreviewValues, DbOperationError> {
    let expected_columns = preview_result_columns(columns, identity_columns);
    if result.values.is_empty() {
        if result.columns.is_empty() || result.columns == expected_columns {
            return Ok(ConvertedPreviewValues {
                visible: Vec::new(),
                identity: (!identity_columns.is_empty()).then(Vec::new),
            });
        }
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned an unexpected column count".to_string(),
        ));
    }
    if result.columns != expected_columns {
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned unexpected columns".to_string(),
        ));
    }
    let rows = result
        .values
        .iter()
        .map(|row| {
            if row.len() != expected_columns.len() {
                return Err(DbOperationError::MetadataParseFailed(
                    "MySQL preview returned an unexpected row width".to_string(),
                ));
            }
            row.iter()
                .zip(columns.iter().chain(identity_columns))
                .map(|(value, column)| {
                    let has_binary_charset = column
                        .character_set_name
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case("binary"));
                    Ok(convert_preview_value(
                        value,
                        &column.data_type,
                        has_binary_charset,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (visible, identity) = rows.into_iter().fold(
        (Vec::new(), (!identity_columns.is_empty()).then(Vec::new)),
        |(mut visible, mut identity), row| {
            let (visible_row, identity_row) = row.split_at(columns.len());
            visible.push(visible_row.to_vec());
            if let Some(identity) = identity.as_mut() {
                identity.push(identity_row.to_vec());
            }
            (visible, identity)
        },
    );
    Ok(ConvertedPreviewValues { visible, identity })
}

pub(in crate::adapters::mysql) fn preview_result_columns(
    columns: &[Column],
    identity_columns: &[Column],
) -> Vec<String> {
    columns
        .iter()
        .map(|column| column.name.clone())
        .chain(
            identity_columns
                .iter()
                .enumerate()
                .map(|(index, _)| preview_identity_alias(index)),
        )
        .collect()
}

fn convert_preview_value(
    value: &QueryValue,
    data_type: &str,
    has_binary_charset: bool,
) -> QueryValue {
    let QueryValue::Text(value) = value else {
        return value.clone();
    };
    if (has_binary_charset || is_binary_type(data_type))
        && let Some(bytes) = decode_hex(value)
    {
        return QueryValue::Blob(bytes);
    }
    if is_numeric_type(data_type) && is_sql_numeric_literal(value) {
        return QueryValue::SqlLiteral(value.clone());
    }
    QueryValue::Text(value.clone())
}

fn is_binary_type(data_type: &str) -> bool {
    let name = data_type
        .split(['(', ' '])
        .next()
        .unwrap_or(data_type)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "binary"
            | "varbinary"
            | "tinyblob"
            | "blob"
            | "mediumblob"
            | "longblob"
            | "bit"
            | "geometry"
            | "point"
            | "linestring"
            | "polygon"
            | "multipoint"
            | "multilinestring"
            | "multipolygon"
            | "geometrycollection"
            | "geomcollection"
    )
}

fn is_numeric_type(data_type: &str) -> bool {
    let name = data_type
        .split(['(', ' '])
        .next()
        .unwrap_or(data_type)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "tinyint"
            | "smallint"
            | "mediumint"
            | "int"
            | "integer"
            | "bigint"
            | "decimal"
            | "dec"
            | "numeric"
            | "fixed"
            | "float"
            | "double"
            | "real"
    )
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let hex = value.strip_prefix("0x")?;
    if hex.len() % 2 != 0 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

fn is_sql_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = usize::from(matches!(bytes[0], b'+' | b'-'));
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let has_integer = index > integer_start;
    let mut has_fraction = false;
    if bytes.get(index) == Some(&b'.') {
        has_fraction = true;
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if !has_integer && !has_fraction {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if exponent_start == index {
            return false;
        }
    }
    index == bytes.len()
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::path::PathBuf;

    use super::*;
    use crate::adapters::mysql::test_query_assertions::assert_queries_in_order;
    use crate::domain::ColumnAttributes;

    fn convert_preview_values(
        result: &MySqlResultSet,
        columns: &[Column],
        identity_columns: &[Column],
    ) -> Result<ConvertedPreviewValues, DbOperationError> {
        convert_preview_values_with_binary_charset(result, columns, identity_columns)
    }

    #[cfg(unix)]
    fn fake_preview_mysql(
        metadata_failure: bool,
        delayed_preview_error: bool,
    ) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let program = directory.path().join("mysql");
        let log_path = directory.path().join("mysql.log");
        let columns_result = if metadata_failure {
            r#"printf '%s\n' '<resultset><row><field name="wrong">x</field></row></resultset>'"#
        } else {
            r#"printf '%s\n' '<resultset><row><field name="COLUMN_NAME">id</field><field name="COLUMN_TYPE">int</field><field name="IS_NULLABLE">NO</field><field name="COLUMN_DEFAULT" xsi:nil="true"/><field name="EXTRA">INVISIBLE</field><field name="COLUMN_COMMENT" xsi:nil="true"/><field name="ORDINAL_POSITION">1</field><field name="PRIMARY_KEY_POSITION">1</field><field name="CHARACTER_SET_NAME" xsi:nil="true"/></row><row><field name="COLUMN_NAME">payload</field><field name="COLUMN_TYPE">varbinary(16)</field><field name="IS_NULLABLE">YES</field><field name="COLUMN_DEFAULT" xsi:nil="true"/><field name="EXTRA"></field><field name="COLUMN_COMMENT"></field><field name="ORDINAL_POSITION">2</field><field name="PRIMARY_KEY_POSITION" xsi:nil="true"/><field name="CHARACTER_SET_NAME" xsi:nil="true"/></row></resultset>'"#
        };
        let script = format!(
            r#"#!/bin/sh
log='{log}'
printf 'argv=%s\n' "$*" >> "$log"
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
printf 'process=%s option=%s\n' "$$" "$option" >> "$log"
if [ -e "$option" ]; then printf 'option-exists=yes\n' >> "$log"; else printf 'option-exists=no\n' >> "$log"; fi
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = "$eof" ] && exit 0
  case "$line" in
    ";") ;;
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)'.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
      ;;
    *"SET SESSION TRANSACTION READ ONLY"*) ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_session_marker.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    *INFORMATION_SCHEMA.COLUMNS*)
      {columns_result}
      ;;
    *"LIMIT 2 OFFSET 1"*)
      printf '%s\n' '<resultset><row><field name="payload">0x00FF</field><field name="__sabiql_row_identity_0">1</field></row></resultset>'
      {delayed_preview_error}
      ;;
    *__sabiql_preview_completion*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\([^']*\)' AS __sabiql_preview_completion.*/\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_preview_completion">'"$marker"'</field></row></resultset>'
      ;;
  esac
done
"#,
            log = log_path.display(),
            columns_result = columns_result,
            delayed_preview_error = if delayed_preview_error {
                "sleep 0.05\nprintf '%s\\n' 'ERROR 1054 (42S22): delayed preview error' >&2"
            } else {
                ""
            },
        );
        fs::write(&program, script).expect("fake MySQL program");
        let mut permissions = fs::metadata(&program)
            .expect("fake MySQL metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).expect("fake MySQL permissions");
        (directory, program, log_path)
    }

    #[cfg(unix)]
    fn assert_option_file_removed(log_path: &std::path::Path) {
        let log = fs::read_to_string(log_path).expect("fake MySQL transcript");
        assert!(log.contains("option-exists=yes"), "{log}");
        let option = log
            .lines()
            .find_map(|line| {
                line.strip_prefix("process=")
                    .and_then(|line| line.split_once(" option=").map(|(_, path)| path))
            })
            .expect("option file path");
        assert!(
            !std::path::Path::new(option).exists(),
            "option file remains"
        );
    }

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    fn column(name: &str, data_type: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes: ColumnAttributes::empty(),
            comment: None,
            ordinal_position: 1,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }

    fn binary_charset_column(name: &str, data_type: &str) -> Column {
        let mut column = column(name, data_type);
        column.character_set_name = Some("binary".to_string());
        column
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn preview_metadata_and_rows_share_one_mysql_session() {
        let (_directory, program, log_path) = fake_preview_mysql(false, false);
        let execution = execute_preview_with_program(
            "mysql://preview:secret@localhost:3306/sabiql_test",
            "sabiql_test",
            "items",
            2,
            1,
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await
        .expect("preview execution");

        assert_eq!(
            execution
                .metadata
                .visible_columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            ["payload"]
        );
        assert_eq!(execution.metadata.order_columns, ["id"]);
        assert_eq!(
            execution
                .metadata
                .identity_columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            ["id"]
        );
        assert_eq!(execution.result_set.values.len(), 1);
        assert_eq!(
            execution.result_set.values[0][0],
            QueryValue::Text("0x00FF".into())
        );
        assert_eq!(
            execution.result_set.values[0][1],
            QueryValue::Text("1".into())
        );
        assert_eq!(
            execution.display_query,
            "SELECT `payload` FROM `sabiql_test`.`items` ORDER BY `id` LIMIT 2 OFFSET 1"
        );

        let log = fs::read_to_string(&log_path).expect("fake MySQL transcript");
        assert_eq!(
            log.lines()
                .filter(|line| line.starts_with("process="))
                .count(),
            1
        );
        let argv = log.lines().find(|line| line.starts_with("argv=")).unwrap();
        assert_eq!(argv.matches("--quick").count(), 1, "{argv}");
        assert_eq!(
            argv.matches("--max-allowed-packet=33554432").count(),
            1,
            "{argv}"
        );
        assert_queries_in_order(
            &log,
            &[
                "SET SESSION autocommit=1, completion_type=NO_CHAIN",
                "SET SESSION TRANSACTION READ ONLY",
                "__sabiql_session_marker",
                "__sabiql_sql_mode",
                "__sabiql_probe",
                "INFORMATION_SCHEMA.COLUMNS",
                "LIMIT 2 OFFSET 1",
                "__sabiql_preview_completion",
            ],
        );
        assert!(!log.contains("INFORMATION_SCHEMA.STATISTICS"), "{log}");
        assert_option_file_removed(&log_path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn preview_metadata_failure_never_sends_user_select() {
        let (_directory, program, log_path) = fake_preview_mysql(true, false);
        let result = execute_preview_with_program(
            "mysql://preview:secret@localhost:3306/sabiql_test",
            "sabiql_test",
            "items",
            2,
            1,
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::MetadataParseFailed(_))
        ));
        let log = fs::read_to_string(&log_path).expect("fake MySQL transcript");
        assert!(log.contains("INFORMATION_SCHEMA.COLUMNS"));
        assert!(!log.contains("LIMIT 2 OFFSET 1"), "{log}");
        assert_option_file_removed(&log_path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delayed_error_between_preview_and_completion_frames_is_classified() {
        let (_directory, program, log_path) = fake_preview_mysql(false, true);
        let result = execute_preview_with_program(
            "mysql://preview:secret@localhost:3306/sabiql_test",
            "sabiql_test",
            "items",
            2,
            1,
            OsStr::new(&program),
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::ObjectMissing(details))
                if details.contains("delayed preview error")
        ));
        assert_option_file_removed(&log_path);
    }

    #[test]
    fn preview_conversion_keeps_text_hex_and_decodes_binary_hex() {
        let result = result(
            &["text_value", "binary_value"],
            vec![vec![
                QueryValue::Text("0x41".to_string()),
                QueryValue::Text("0x00FF".to_string()),
            ]],
        );
        let columns = vec![
            column("text_value", "varchar(20)"),
            column("binary_value", "blob"),
        ];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0], QueryValue::Text("0x41".to_string()));
        assert_eq!(values.visible[0][1], QueryValue::Blob(vec![0, 255]));
    }

    #[test]
    fn preview_conversion_decodes_binary_character_set_hex() {
        let result = result(
            &["char_value", "varchar_value", "text_value"],
            vec![vec![
                QueryValue::Text("0x00FFA1".to_string()),
                QueryValue::Text("0x10FE".to_string()),
                QueryValue::Text("0x41".to_string()),
            ]],
        );
        let columns = vec![
            binary_charset_column("char_value", "char(3)"),
            binary_charset_column("varchar_value", "varchar(3)"),
            column("text_value", "varchar(3)"),
        ];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0], QueryValue::Blob(vec![0, 255, 161]));
        assert_eq!(values.visible[0][1], QueryValue::Blob(vec![16, 254]));
        assert_eq!(values.visible[0][2], QueryValue::Text("0x41".to_string()));
    }

    #[test]
    fn preview_conversion_decodes_bit_hex_as_binary() {
        let result = result(
            &["bit_value"],
            vec![vec![QueryValue::Text("0x05".to_string())]],
        );
        let columns = vec![column("bit_value", "bit(3)")];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0], QueryValue::Blob(vec![5]));
    }

    #[test]
    fn preview_conversion_decodes_spatial_hex_as_binary() {
        let result = result(
            &["location"],
            vec![vec![QueryValue::Text(
                "0xE610000001010000000000000000805E40CDCCCCCCCCCC4240".to_string(),
            )]],
        );
        let columns = vec![column("location", "point srid 4326")];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(
            values.visible[0][0],
            QueryValue::Blob(vec![
                0xE6, 0x10, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x80, 0x5E, 0x40, 0xCD, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x42, 0x40,
            ])
        );
    }

    #[test]
    fn preview_conversion_keeps_numeric_server_literals_without_rounding() {
        let result = result(
            &["unsigned_value", "decimal_value", "float_value"],
            vec![vec![
                QueryValue::Text("18446744073709551615".to_string()),
                QueryValue::Text("12345678901234567890.12345678901234567890".to_string()),
                QueryValue::Text("1.23e+100".to_string()),
            ]],
        );
        let columns = vec![
            column("unsigned_value", "bigint unsigned"),
            column("decimal_value", "decimal(65,30)"),
            column("float_value", "double"),
        ];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0].as_str(), Some("18446744073709551615"));
        assert!(matches!(values.visible[0][0], QueryValue::SqlLiteral(_)));
        assert!(matches!(values.visible[0][1], QueryValue::SqlLiteral(_)));
        assert!(matches!(values.visible[0][2], QueryValue::SqlLiteral(_)));
    }

    #[test]
    fn accepts_valid_decimal_and_rejects_incomplete_numeric_literals() {
        assert!(is_sql_numeric_literal("1."));
        assert!(is_sql_numeric_literal(".5"));
        assert!(is_sql_numeric_literal("-1.2e-3"));
        assert!(!is_sql_numeric_literal("1e"));
        assert!(!is_sql_numeric_literal("nan"));
    }

    #[test]
    fn preview_conversion_separates_aliased_primary_key_values() {
        let result = result(
            &["payload", "__sabiql_row_identity_0"],
            vec![vec![
                QueryValue::Text("visible".to_string()),
                QueryValue::Text("42".to_string()),
            ]],
        );
        let visible_columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];

        let values = convert_preview_values(&result, &visible_columns, &identity_columns)
            .expect("conversion succeeds");

        assert_eq!(
            values.visible,
            vec![vec![QueryValue::Text("visible".to_string())]]
        );
        assert_eq!(
            values.identity,
            Some(vec![vec![QueryValue::SqlLiteral("42".to_string())]])
        );
    }

    #[test]
    fn preview_conversion_preserves_empty_result_shape_with_identity_columns() {
        let columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];
        let result = result(&["payload", "__sabiql_row_identity_0"], Vec::new());

        let values = convert_preview_values(&result, &columns, &identity_columns)
            .expect("empty result conversion");

        assert!(values.visible.is_empty());
        assert_eq!(values.identity, Some(Vec::new()));
    }
}
