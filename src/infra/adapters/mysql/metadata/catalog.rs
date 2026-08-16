use std::collections::HashSet;
use std::ffi::OsStr;
use std::time::Duration;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    Column, ColumnAttributes, FkAction, ForeignKey, QueryValue, TableKind, TableKindInfo,
    TableSummary,
};

use super::super::{
    cli::{MYSQL_QUERY_TIMEOUT, MySqlMetadataSession, MySqlResultSet},
    dsn::parse_and_validate_mysql_dsn,
    option_file::MySqlOptionFile,
    sql::{
        COLUMN_METADATA_RESULT_COLUMNS, FOREIGN_KEY_RESULT_COLUMNS,
        PREVIEW_COLUMN_METADATA_RESULT_COLUMNS, TABLES_QUERY, TABLES_RESULT_COLUMNS,
        UNIQUE_COLUMN_RESULT_COLUMNS,
    },
};

#[derive(Debug, Clone)]
enum MySqlColumnUnique {
    None,
    SingleColumnIndex,
}

#[derive(Debug, Clone)]
pub(super) struct MySqlColumnMetadata {
    name: String,
    data_type: String,
    character_set_name: Option<String>,
    nullable: bool,
    default: Option<String>,
    comment: Option<String>,
    pub(super) ordinal_position: i32,
    primary_key_position: Option<i32>,
    unique: MySqlColumnUnique,
    invisible: bool,
    generated: bool,
}

impl MySqlColumnMetadata {
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn has_binary_character_set(&self) -> bool {
        self.character_set_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("binary"))
    }
}

#[derive(Debug, Clone)]
pub(super) struct MySqlForeignKeyMetadata {
    pub(super) name: String,
    pub(super) from_schema: String,
    pub(super) from_table: String,
    pub(super) from_column: String,
    pub(super) to_schema: String,
    pub(super) to_table: String,
    pub(super) to_column: String,
    pub(super) ordinal_position: i32,
    pub(super) on_update: FkAction,
    pub(super) on_delete: FkAction,
}

#[derive(Debug, Clone)]
pub(super) struct MySqlTableMetadata {
    pub(super) schema: String,
    pub(super) name: String,
    pub(super) kind: TableKind,
    pub(super) row_count_estimate: Option<i64>,
    pub(super) comment: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct MySqlMetadataSnapshot {
    pub(super) database: String,
    pub(super) tables: Vec<MySqlTableMetadata>,
    pub(super) table_summaries: Vec<TableSummary>,
}

pub(super) async fn fetch_metadata_snapshot(
    dsn: &str,
) -> Result<MySqlMetadataSnapshot, DbOperationError> {
    let database = selected_database(dsn)?;
    let result = execute_metadata_query(dsn, TABLES_QUERY, TABLES_RESULT_COLUMNS).await?;
    metadata_snapshot_from_result(&database, None, &result)
}

pub(super) fn find_table(
    schema: &str,
    table: &str,
    tables: &[MySqlTableMetadata],
) -> Result<MySqlTableMetadata, DbOperationError> {
    tables
        .iter()
        .find(|candidate| candidate.schema == schema && candidate.name == table)
        .cloned()
        .ok_or_else(|| {
            DbOperationError::ObjectMissing(format!("MySQL table not found: {schema}.{table}"))
        })
}

pub(super) fn parse_columns_for_table(
    result: &MySqlResultSet,
    schema: &str,
    table: &str,
) -> Result<Vec<MySqlColumnMetadata>, DbOperationError> {
    let columns = parse_column_metadata(result)?;
    if columns.is_empty() {
        return Err(DbOperationError::ObjectMissing(format!(
            "MySQL table not found: {schema}.{table}"
        )));
    }
    Ok(columns)
}

pub(super) fn parse_preview_columns_for_table(
    result: &MySqlResultSet,
    schema: &str,
    table: &str,
) -> Result<Vec<MySqlColumnMetadata>, DbOperationError> {
    expect_columns(result, PREVIEW_COLUMN_METADATA_RESULT_COLUMNS)?;
    let columns = result
        .values
        .iter()
        .map(|row| {
            if row.len() != PREVIEW_COLUMN_METADATA_RESULT_COLUMNS.len() {
                return Err(metadata_shape_error("preview COLUMNS row"));
            }
            let mut column = parse_column_metadata_row(
                &row[..COLUMN_METADATA_RESULT_COLUMNS.len()],
                "preview COLUMNS row",
            )?;
            column.character_set_name = optional_text(
                &row[COLUMN_METADATA_RESULT_COLUMNS.len()],
                "CHARACTER_SET_NAME",
            )?
            .map(str::to_string);
            Ok(column)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if columns.is_empty() {
        return Err(DbOperationError::ObjectMissing(format!(
            "MySQL table not found: {schema}.{table}"
        )));
    }
    Ok(columns)
}

pub(super) async fn execute_metadata_query(
    dsn: &str,
    query: &str,
    expected_columns: &[&str],
) -> Result<MySqlResultSet, DbOperationError> {
    execute_metadata_queries_in_session(dsn, &[(query, expected_columns)])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            DbOperationError::MetadataParseFailed(
                "MySQL metadata query returned no result set".to_string(),
            )
        })
}

pub(super) async fn execute_metadata_queries_in_session(
    dsn: &str,
    queries: &[(&str, &[&str])],
) -> Result<Vec<MySqlResultSet>, DbOperationError> {
    execute_metadata_queries_in_session_with_program(
        dsn,
        queries,
        OsStr::new("mysql"),
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn execute_metadata_queries_in_session_with_program(
    dsn: &str,
    queries: &[(&str, &[&str])],
    program: &OsStr,
    timeout: Duration,
) -> Result<Vec<MySqlResultSet>, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut session = MySqlMetadataSession::spawn_with_program(program, &option_file.path)?;
    let result = tokio::time::timeout(timeout, async {
        session.probe().await?;
        session.prepare_read_only().await?;
        let mut results = Vec::with_capacity(queries.len());
        for (query, expected_columns) in queries {
            results.push(
                session
                    .execute_with_expected_columns(query, expected_columns)
                    .await?,
            );
        }
        session.finish().await?;
        Ok(results)
    })
    .await;
    let result = match result {
        Ok(Ok(results)) => Ok(results),
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

pub(super) fn selected_database(dsn: &str) -> Result<String, DbOperationError> {
    parse_and_validate_mysql_dsn(dsn)?.database.ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "MySQL metadata requires a selected database".to_string(),
        )
    })
}

pub(super) fn validate_selected_schema_name(
    database: &str,
    schema: &str,
) -> Result<(), DbOperationError> {
    if schema != database {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn metadata_snapshot_from_result(
    database: &str,
    requested_schema: Option<&str>,
    result: &MySqlResultSet,
) -> Result<MySqlMetadataSnapshot, DbOperationError> {
    if let Some(schema) = requested_schema {
        validate_selected_schema_name(database, schema)?;
    }
    let tables = parse_table_metadata(result)?;
    let table_summaries = tables.iter().cloned().map(table_summary).collect();
    Ok(MySqlMetadataSnapshot {
        database: database.to_string(),
        tables,
        table_summaries,
    })
}

fn parse_table_metadata(
    result: &MySqlResultSet,
) -> Result<Vec<MySqlTableMetadata>, DbOperationError> {
    expect_columns(result, TABLES_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 5 {
                return Err(metadata_shape_error("TABLES row"));
            }
            let schema = required_text(&row[0], "TABLE_SCHEMA")?.to_string();
            let name = required_text(&row[1], "TABLE_NAME")?.to_string();
            let kind = match required_text(&row[2], "TABLE_TYPE")? {
                "BASE TABLE" => TableKind::Table,
                "VIEW" => TableKind::View,
                value => {
                    return Err(DbOperationError::MetadataParseFailed(format!(
                        "unexpected MySQL table type: {value}"
                    )));
                }
            };
            let row_count_estimate = optional_text(&row[3], "TABLE_ROWS")?
                .map(|value| {
                    value.parse::<i64>().map_err(|_| {
                        DbOperationError::MetadataParseFailed(
                            "invalid MySQL TABLE_ROWS value".to_string(),
                        )
                    })
                })
                .transpose()?;
            let comment = optional_text(&row[4], "TABLE_COMMENT")?
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Ok(MySqlTableMetadata {
                schema,
                name,
                kind,
                row_count_estimate,
                comment,
            })
        })
        .collect()
}

fn parse_column_metadata(
    result: &MySqlResultSet,
) -> Result<Vec<MySqlColumnMetadata>, DbOperationError> {
    expect_columns(result, COLUMN_METADATA_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 8 {
                return Err(metadata_shape_error("COLUMNS row"));
            }
            parse_column_metadata_row(row, "COLUMNS row")
        })
        .collect()
}

pub(super) fn parse_column_metadata_row(
    row: &[QueryValue],
    field: &str,
) -> Result<MySqlColumnMetadata, DbOperationError> {
    if row.len() != 8 {
        return Err(metadata_shape_error(field));
    }
    let extra = required_text(&row[4], "EXTRA")?;
    Ok(MySqlColumnMetadata {
        name: required_text(&row[0], "COLUMN_NAME")?.to_string(),
        data_type: required_text(&row[1], "COLUMN_TYPE")?.to_string(),
        character_set_name: None,
        nullable: required_text(&row[2], "IS_NULLABLE")? == "YES",
        default: optional_text(&row[3], "COLUMN_DEFAULT")?.map(str::to_string),
        comment: optional_text(&row[5], "COLUMN_COMMENT")?
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        ordinal_position: parse_positive_i32(&row[6], "ORDINAL_POSITION")?,
        primary_key_position: optional_text(&row[7], "PRIMARY_KEY_POSITION")?
            .map(|value| parse_positive_i32_text(value, "PRIMARY_KEY_POSITION"))
            .transpose()?,
        unique: MySqlColumnUnique::None,
        invisible: extra
            .split_ascii_whitespace()
            .any(|word| word.eq_ignore_ascii_case("INVISIBLE")),
        generated: extra
            .split_ascii_whitespace()
            .any(|word| word.eq_ignore_ascii_case("GENERATED")),
    })
}

pub(super) fn parse_foreign_key_metadata(
    result: &MySqlResultSet,
) -> Result<Vec<MySqlForeignKeyMetadata>, DbOperationError> {
    expect_columns(result, FOREIGN_KEY_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 10 {
                return Err(metadata_shape_error("foreign key row"));
            }
            Ok(MySqlForeignKeyMetadata {
                name: required_text(&row[0], "CONSTRAINT_NAME")?.to_string(),
                from_schema: required_text(&row[1], "TABLE_SCHEMA")?.to_string(),
                from_table: required_text(&row[2], "TABLE_NAME")?.to_string(),
                from_column: required_text(&row[3], "COLUMN_NAME")?.to_string(),
                to_schema: required_text(&row[4], "REFERENCED_TABLE_SCHEMA")?.to_string(),
                to_table: required_text(&row[5], "REFERENCED_TABLE_NAME")?.to_string(),
                to_column: required_text(&row[6], "REFERENCED_COLUMN_NAME")?.to_string(),
                ordinal_position: parse_positive_i32(&row[7], "ORDINAL_POSITION")?,
                on_update: parse_fk_action(&row[8], "UPDATE_RULE")?,
                on_delete: parse_fk_action(&row[9], "DELETE_RULE")?,
            })
        })
        .collect()
}

pub(super) fn foreign_keys_from_metadata(
    mut raw: Vec<MySqlForeignKeyMetadata>,
    database: &str,
) -> Result<Vec<ForeignKey>, DbOperationError> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut foreign_keys = Vec::new();
    for column in raw {
        let reference_resolved = column.to_schema == database;
        if let Some(foreign_key) = foreign_keys
            .iter_mut()
            .find(|foreign_key: &&mut ForeignKey| foreign_key.name == column.name)
        {
            if foreign_key.from_schema != column.from_schema
                || foreign_key.from_table != column.from_table
                || foreign_key.to_schema != column.to_schema
                || foreign_key.to_table != column.to_table
                || foreign_key.on_update != column.on_update
                || foreign_key.on_delete != column.on_delete
            {
                return Err(metadata_shape_error("foreign key constraint columns"));
            }
            foreign_key.from_columns.push(column.from_column);
            foreign_key.to_columns.push(column.to_column);
            foreign_key.reference_resolved &= reference_resolved;
            continue;
        }
        foreign_keys.push(ForeignKey {
            name: column.name,
            from_schema: column.from_schema,
            from_table: column.from_table,
            from_columns: vec![column.from_column],
            to_schema: column.to_schema,
            to_table: column.to_table,
            to_columns: vec![column.to_column],
            on_delete: column.on_delete,
            on_update: column.on_update,
            reference_resolved,
        });
    }
    Ok(foreign_keys)
}

pub(super) fn column_from_metadata(metadata: &MySqlColumnMetadata) -> Column {
    let primary_key = metadata.primary_key_position.is_some();
    let attributes = ColumnAttributes::from_parts(
        metadata.nullable,
        primary_key,
        matches!(metadata.unique, MySqlColumnUnique::SingleColumnIndex),
    ) | if metadata.invisible {
        ColumnAttributes::HIDDEN | ColumnAttributes::READ_ONLY
    } else {
        ColumnAttributes::empty()
    } | if metadata.generated {
        ColumnAttributes::GENERATED | ColumnAttributes::READ_ONLY
    } else {
        ColumnAttributes::empty()
    };
    Column {
        name: metadata.name.clone(),
        data_type: metadata.data_type.clone(),
        default: metadata.default.clone(),
        attributes,
        comment: metadata.comment.clone(),
        ordinal_position: metadata.ordinal_position,
    }
}

pub(super) fn parse_unique_column_metadata(
    result: &MySqlResultSet,
) -> Result<HashSet<String>, DbOperationError> {
    expect_columns(result, UNIQUE_COLUMN_RESULT_COLUMNS)?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 1 {
                return Err(metadata_shape_error("single-column UNIQUE row"));
            }
            Ok(required_text(&row[0], "COLUMN_NAME")?.to_string())
        })
        .collect()
}

pub(super) fn mark_single_column_unique(
    columns: &mut [MySqlColumnMetadata],
    unique_columns: &HashSet<String>,
) {
    for column in columns {
        column.unique = if unique_columns.contains(&column.name) {
            MySqlColumnUnique::SingleColumnIndex
        } else {
            MySqlColumnUnique::None
        };
    }
}

pub(super) fn primary_key_names(columns: &[MySqlColumnMetadata]) -> Vec<String> {
    let mut columns = columns
        .iter()
        .filter_map(|column| {
            column
                .primary_key_position
                .map(|position| (position, column.name.clone()))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(position, _)| *position);
    columns.into_iter().map(|(_, name)| name).collect()
}

pub(super) fn parse_boolean_flag(
    value: &QueryValue,
    field: &str,
) -> Result<bool, DbOperationError> {
    match required_text(value, field)? {
        "0" | "NO" => Ok(false),
        "1" | "YES" => Ok(true),
        _ => Err(DbOperationError::MetadataParseFailed(format!(
            "invalid MySQL metadata boolean: {field}"
        ))),
    }
}

fn parse_fk_action(value: &QueryValue, field: &str) -> Result<FkAction, DbOperationError> {
    required_text(value, field)?.parse().map_err(|error| {
        DbOperationError::MetadataParseFailed(format!("invalid MySQL foreign key action: {error}"))
    })
}

fn table_summary(table: MySqlTableMetadata) -> TableSummary {
    TableSummary::new(table.schema, table.name, table.row_count_estimate, false).with_kind_info(
        TableKindInfo {
            kind: table.kind,
            is_strict: false,
            without_rowid: false,
            virtual_module: None,
        },
    )
}

pub(super) fn expect_columns(
    result: &MySqlResultSet,
    expected: &[&str],
) -> Result<(), DbOperationError> {
    if result.columns == expected {
        Ok(())
    } else {
        Err(metadata_shape_error("MySQL metadata columns"))
    }
}

pub(super) fn required_text<'a>(
    value: &'a QueryValue,
    field: &str,
) -> Result<&'a str, DbOperationError> {
    optional_text(value, field)?.ok_or_else(|| {
        DbOperationError::MetadataParseFailed(format!("MySQL metadata field is NULL: {field}"))
    })
}

pub(super) fn optional_text<'a>(
    value: &'a QueryValue,
    field: &str,
) -> Result<Option<&'a str>, DbOperationError> {
    match value {
        QueryValue::Null => Ok(None),
        QueryValue::Text(value) | QueryValue::SqlLiteral(value) => Ok(Some(value)),
        QueryValue::Blob(_) => Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL metadata field is binary: {field}"
        ))),
    }
}

pub(super) fn parse_positive_i32(value: &QueryValue, field: &str) -> Result<i32, DbOperationError> {
    parse_positive_i32_text(required_text(value, field)?, field)
}

pub(super) fn parse_optional_positive_i32(
    value: &QueryValue,
    field: &str,
) -> Result<Option<i32>, DbOperationError> {
    optional_text(value, field)?
        .map(|value| {
            let value = value.parse::<i32>().map_err(|_| {
                DbOperationError::MetadataParseFailed(format!(
                    "invalid MySQL metadata integer: {field}"
                ))
            })?;
            (value > 0).then_some(value).ok_or_else(|| {
                DbOperationError::MetadataParseFailed(format!(
                    "invalid MySQL metadata integer: {field}"
                ))
            })
        })
        .transpose()
}

fn parse_positive_i32_text(value: &str, field: &str) -> Result<i32, DbOperationError> {
    let value = value.parse::<i32>().map_err(|_| {
        DbOperationError::MetadataParseFailed(format!("invalid MySQL metadata integer: {field}"))
    })?;
    if value > 0 {
        Ok(value)
    } else {
        Err(DbOperationError::MetadataParseFailed(format!(
            "invalid MySQL metadata ordinal: {field}"
        )))
    }
}

pub(super) fn metadata_shape_error(field: &str) -> DbOperationError {
    DbOperationError::MetadataParseFailed(format!("unexpected MySQL metadata shape: {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    #[test]
    fn metadata_snapshot_uses_server_schema() {
        let snapshot = metadata_snapshot_from_result(
            "app",
            Some("app"),
            &result(
                &[
                    "TABLE_SCHEMA",
                    "TABLE_NAME",
                    "TABLE_TYPE",
                    "TABLE_ROWS",
                    "TABLE_COMMENT",
                ],
                vec![vec![
                    QueryValue::Text("app".to_string()),
                    QueryValue::Text("users".to_string()),
                    QueryValue::Text("BASE TABLE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Null,
                ]],
            ),
        )
        .unwrap();

        assert_eq!(snapshot.table_summaries[0].name, "users");
        assert_eq!(snapshot.table_summaries[0].schema, "app");
    }

    #[test]
    fn metadata_rejects_database_with_different_case() {
        let error =
            metadata_snapshot_from_result("app", Some("APP"), &result(&[], vec![])).unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::UnsupportedOperation(message)
                if message == "MySQL metadata is limited to the selected database"
        ));
    }

    #[test]
    fn metadata_schema_mismatch_rejects_before_parsing() {
        let error =
            metadata_snapshot_from_result("app", Some("other"), &result(&[], vec![])).unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::UnsupportedOperation(message)
                if message == "MySQL metadata is limited to the selected database"
        ));
    }

    #[test]
    fn find_table_rejects_schema_with_different_case() {
        let error = find_table(
            "APP",
            "users",
            &[MySqlTableMetadata {
                schema: "app".to_string(),
                name: "users".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
            }],
        )
        .unwrap_err();

        assert!(matches!(error, DbOperationError::ObjectMissing(_)));
    }

    #[test]
    fn metadata_parser_preserves_column_and_composite_key_order() {
        let result = result(
            &[
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
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("2".to_string()),
                ],
                vec![
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
                vec![
                    QueryValue::Text("generated_value".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("STORED GENERATED".to_string()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("3".to_string()),
                    QueryValue::Null,
                ],
            ],
        );

        let parsed = parse_column_metadata(&result).expect("metadata parses");
        assert_eq!(primary_key_names(&parsed), ["second_key", "first_key"]);
        assert_eq!(
            parsed
                .iter()
                .map(|column| column.ordinal_position)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(column_from_metadata(&parsed[2]).is_generated());
    }

    #[test]
    fn metadata_parser_marks_invisible_columns_hidden_and_read_only() {
        let parsed = parse_column_metadata(&result(
            COLUMN_METADATA_RESULT_COLUMNS,
            vec![vec![
                QueryValue::Text("hidden_value".to_string()),
                QueryValue::Text("varchar(20)".to_string()),
                QueryValue::Text("YES".to_string()),
                QueryValue::Null,
                QueryValue::Text("INVISIBLE".to_string()),
                QueryValue::Null,
                QueryValue::Text("1".to_string()),
                QueryValue::Null,
            ]],
        ))
        .expect("metadata parses");

        let column = column_from_metadata(&parsed[0]);

        assert!(column.is_hidden());
        assert!(column.is_read_only());
    }

    #[test]
    fn preview_metadata_preserves_binary_character_set() {
        let parsed = parse_preview_columns_for_table(
            &result(
                PREVIEW_COLUMN_METADATA_RESULT_COLUMNS,
                vec![
                    vec![
                        QueryValue::Text("binary_char".to_string()),
                        QueryValue::Text("char(4)".to_string()),
                        QueryValue::Text("NO".to_string()),
                        QueryValue::Null,
                        QueryValue::Text(String::new()),
                        QueryValue::Null,
                        QueryValue::Text("1".to_string()),
                        QueryValue::Text("1".to_string()),
                        QueryValue::Text("binary".to_string()),
                    ],
                    vec![
                        QueryValue::Text("normal_text".to_string()),
                        QueryValue::Text("varchar(4)".to_string()),
                        QueryValue::Text("NO".to_string()),
                        QueryValue::Null,
                        QueryValue::Text(String::new()),
                        QueryValue::Null,
                        QueryValue::Text("2".to_string()),
                        QueryValue::Null,
                        QueryValue::Text("utf8mb4".to_string()),
                    ],
                ],
            ),
            "app",
            "items",
        )
        .expect("preview metadata parses");

        assert!(parsed[0].has_binary_character_set());
        assert!(!parsed[1].has_binary_character_set());
    }

    #[test]
    fn empty_columns_result_maps_to_missing_table() {
        let result = result(COLUMN_METADATA_RESULT_COLUMNS, Vec::new());

        let error = parse_columns_for_table(&result, "app", "missing").unwrap_err();
        assert!(matches!(error, DbOperationError::ObjectMissing(_)));
    }

    #[test]
    fn single_column_unique_metadata_sets_only_matching_column_attribute() {
        let columns = parse_column_metadata(&result(
            COLUMN_METADATA_RESULT_COLUMNS,
            vec![
                vec![
                    QueryValue::Text("id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
                vec![
                    QueryValue::Text("email".to_string()),
                    QueryValue::Text("varchar(255)".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("2".to_string()),
                    QueryValue::Null,
                ],
            ],
        ))
        .unwrap();
        let unique_columns = parse_unique_column_metadata(&result(
            UNIQUE_COLUMN_RESULT_COLUMNS,
            vec![vec![QueryValue::Text("email".to_string())]],
        ))
        .unwrap();
        let mut columns = columns;

        mark_single_column_unique(&mut columns, &unique_columns);

        assert!(!column_from_metadata(&columns[0]).is_unique());
        assert!(column_from_metadata(&columns[1]).is_unique());
    }

    #[test]
    fn groups_foreign_keys_by_name_and_orders_columns_by_sequence() {
        let result = result(
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
            vec![
                vec![
                    QueryValue::Text("fk_child_parent".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_second".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("CASCADE".to_string()),
                    QueryValue::Text("SET NULL".to_string()),
                ],
                vec![
                    QueryValue::Text("fk_child_parent".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_first".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("CASCADE".to_string()),
                    QueryValue::Text("SET NULL".to_string()),
                ],
            ],
        );

        let foreign_keys =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), "sabiql_test")
                .unwrap();

        assert_eq!(foreign_keys.len(), 1);
        assert_eq!(
            foreign_keys[0].from_columns,
            ["parent_first", "parent_second"]
        );
        assert_eq!(foreign_keys[0].to_columns, ["first_key", "second_key"]);
        assert_eq!(foreign_keys[0].on_update, FkAction::Cascade);
        assert_eq!(foreign_keys[0].on_delete, FkAction::SetNull);
        assert!(foreign_keys[0].is_reference_resolved());

        let unresolved =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), "SABIQL_TEST")
                .unwrap();
        assert!(!unresolved[0].is_reference_resolved());
    }
}
