use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{
    Column, ColumnAttributes, FkAction, ForeignKey, QueryValue, TableKind, TableKindInfo,
    TableSummary,
};

use super::super::{
    cli::{MysqlResultSet, run_mysql_adhoc, validate_mysql_multi_query},
    dsn::parse_and_validate_mysql_dsn,
    option_file::MySqlOptionFile,
    sql::quote_string,
};

pub(super) const TABLES_QUERY: &str = "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') UNION ALL SELECT NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')) ORDER BY TABLE_SCHEMA, TABLE_NAME";

#[derive(Debug, Clone)]
pub(super) struct MysqlColumnMetadata {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    comment: Option<String>,
    pub(super) ordinal_position: i32,
    primary_key_position: Option<i32>,
    invisible: bool,
    generated: bool,
}

#[derive(Debug, Clone)]
pub(super) struct MysqlForeignKeyMetadata {
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
pub(super) struct MysqlTableMetadata {
    pub(super) schema: String,
    pub(super) name: String,
    pub(super) kind: TableKind,
    pub(super) row_count_estimate: Option<i64>,
    pub(super) comment: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct MysqlMetadataSnapshot {
    pub(super) database: String,
    pub(super) tables: Vec<MysqlTableMetadata>,
    pub(super) table_summaries: Vec<TableSummary>,
}

pub(super) async fn fetch_metadata_snapshot(
    dsn: &str,
) -> Result<MysqlMetadataSnapshot, DbOperationError> {
    let database = selected_database(dsn)?;
    let result = execute_metadata_query(dsn, TABLES_QUERY).await?;
    metadata_snapshot_from_result(&database, None, &result)
}

pub(super) async fn fetch_table_metadata(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<MysqlMetadataSnapshot, DbOperationError> {
    let database = selected_database(dsn)?;
    validate_selected_schema_name(&database, schema)?;
    let result = execute_metadata_query(dsn, &table_query(schema, table)).await?;
    metadata_snapshot_from_result(&database, Some(schema), &result)
}

pub(super) async fn fetch_columns(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    validate_selected_schema(dsn, schema)?;
    let result = execute_metadata_query(dsn, &columns_query(schema, table)).await?;
    let columns = parse_columns_for_table(&result, schema, table)?;
    Ok(columns)
}

pub(super) async fn fetch_foreign_keys(
    dsn: &str,
    schema: &str,
    table: &str,
    summaries: &[TableSummary],
) -> Result<Vec<ForeignKey>, DbOperationError> {
    validate_selected_schema(dsn, schema)?;
    let result = execute_metadata_query(dsn, &foreign_keys_query(schema, table)).await?;
    let raw = parse_foreign_key_metadata(&result)?;
    foreign_keys_from_metadata(raw, summaries)
}

pub(super) fn find_table(
    schema: &str,
    table: &str,
    tables: &[MysqlTableMetadata],
) -> Result<MysqlTableMetadata, DbOperationError> {
    tables
        .iter()
        .find(|candidate| candidate.schema.eq_ignore_ascii_case(schema) && candidate.name == table)
        .cloned()
        .ok_or_else(|| {
            DbOperationError::ObjectMissing(format!("MySQL table not found: {schema}.{table}"))
        })
}

pub(super) fn table_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND (t.TABLE_NAME = {} OR EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = {} AND tc.TABLE_SCHEMA = {} AND tc.TABLE_NAME = {} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' AND kcu.REFERENCED_TABLE_SCHEMA = t.TABLE_SCHEMA AND kcu.REFERENCED_TABLE_NAME = t.TABLE_NAME)) UNION ALL SELECT NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {}) ORDER BY TABLE_SCHEMA, TABLE_NAME",
        quote_string(schema),
        quote_string(table),
        quote_string(schema),
        quote_string(schema),
        quote_string(table),
        quote_string(schema),
        quote_string(table),
    )
}

pub(super) fn columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {} AND c.TABLE_NAME = {} UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {}) ORDER BY ORDINAL_POSITION",
        quote_string(schema),
        quote_string(table),
        quote_string(schema),
        quote_string(table),
    )
}

pub(super) fn parse_columns_for_table(
    result: &MysqlResultSet,
    schema: &str,
    table: &str,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    let columns = parse_column_metadata(result)?;
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
) -> Result<MysqlResultSet, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let statements =
        validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadOnly)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let result = run_mysql_adhoc(&option_file.path, query, &statements, AccessMode::ReadOnly).await;
    drop(option_file);
    result?.result_set.ok_or_else(|| {
        DbOperationError::MetadataParseFailed(
            "MySQL metadata query returned no result set".to_string(),
        )
    })
}

fn selected_database(dsn: &str) -> Result<String, DbOperationError> {
    parse_and_validate_mysql_dsn(dsn)?.database.ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "MySQL metadata requires a selected database".to_string(),
        )
    })
}

fn validate_selected_schema(dsn: &str, schema: &str) -> Result<(), DbOperationError> {
    validate_selected_schema_name(&selected_database(dsn)?, schema)
}

pub(super) fn validate_selected_schema_name(
    database: &str,
    schema: &str,
) -> Result<(), DbOperationError> {
    if !schema.eq_ignore_ascii_case(database) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn metadata_snapshot_from_result(
    database: &str,
    requested_schema: Option<&str>,
    result: &MysqlResultSet,
) -> Result<MysqlMetadataSnapshot, DbOperationError> {
    if let Some(schema) = requested_schema {
        validate_selected_schema_name(database, schema)?;
    }
    let tables = parse_table_metadata(result)?;
    let table_summaries = tables.iter().cloned().map(table_summary).collect();
    Ok(MysqlMetadataSnapshot {
        database: database.to_string(),
        tables,
        table_summaries,
    })
}

fn parse_table_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlTableMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "TABLE_SCHEMA",
            "TABLE_NAME",
            "TABLE_TYPE",
            "TABLE_ROWS",
            "TABLE_COMMENT",
        ],
    )?;
    let tables = result
        .values
        .iter()
        .map(|row| {
            if row.len() != 5 {
                return Err(metadata_shape_error("TABLES row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
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
            Ok(Some(MysqlTableMetadata {
                schema,
                name,
                kind,
                row_count_estimate,
                comment,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tables.into_iter().flatten().collect())
}

pub(super) fn foreign_keys_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = {} AND tc.TABLE_SCHEMA = {} AND tc.TABLE_NAME = {} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = {} AND TABLE_SCHEMA = {} AND TABLE_NAME = {} AND CONSTRAINT_TYPE = 'FOREIGN KEY') ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
        quote_string(schema),
        quote_string(schema),
        quote_string(table),
        quote_string(schema),
        quote_string(schema),
        quote_string(table),
    )
}

fn parse_column_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    expect_columns(
        result,
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
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 8 {
                return Err(metadata_shape_error("COLUMNS row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            Ok(Some(parse_column_metadata_row(row, "COLUMNS row")?))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

pub(super) fn parse_column_metadata_row(
    row: &[QueryValue],
    field: &str,
) -> Result<MysqlColumnMetadata, DbOperationError> {
    if row.len() != 8 {
        return Err(metadata_shape_error(field));
    }
    let extra = required_text(&row[4], "EXTRA")?;
    Ok(MysqlColumnMetadata {
        name: required_text(&row[0], "COLUMN_NAME")?.to_string(),
        data_type: required_text(&row[1], "COLUMN_TYPE")?.to_string(),
        nullable: required_text(&row[2], "IS_NULLABLE")? == "YES",
        default: optional_text(&row[3], "COLUMN_DEFAULT")?.map(str::to_string),
        comment: optional_text(&row[5], "COLUMN_COMMENT")?
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        ordinal_position: parse_positive_i32(&row[6], "ORDINAL_POSITION")?,
        primary_key_position: optional_text(&row[7], "PRIMARY_KEY_POSITION")?
            .map(|value| parse_positive_i32_text(value, "PRIMARY_KEY_POSITION"))
            .transpose()?,
        invisible: extra
            .split_ascii_whitespace()
            .any(|word| word.eq_ignore_ascii_case("INVISIBLE")),
        generated: extra
            .split_ascii_whitespace()
            .any(|word| word.eq_ignore_ascii_case("GENERATED")),
    })
}

pub(super) fn parse_foreign_key_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlForeignKeyMetadata>, DbOperationError> {
    expect_columns(
        result,
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
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            if row.len() != 10 {
                return Err(metadata_shape_error("foreign key row"));
            }
            Ok(Some(MysqlForeignKeyMetadata {
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
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

pub(super) fn foreign_keys_from_metadata(
    mut raw: Vec<MysqlForeignKeyMetadata>,
    summaries: &[TableSummary],
) -> Result<Vec<ForeignKey>, DbOperationError> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut foreign_keys = Vec::new();
    for column in raw {
        let reference_resolved = summaries
            .iter()
            .any(|summary| summary.schema == column.to_schema && summary.name == column.to_table);
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

pub(super) fn column_from_metadata(metadata: &MysqlColumnMetadata) -> Column {
    let primary_key = metadata.primary_key_position.is_some();
    let attributes = ColumnAttributes::from_parts(metadata.nullable, primary_key, false)
        | if metadata.invisible {
            ColumnAttributes::HIDDEN | ColumnAttributes::READ_ONLY
        } else {
            ColumnAttributes::empty()
        }
        | if metadata.generated {
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

pub(super) fn primary_key_names(columns: &[MysqlColumnMetadata]) -> Vec<String> {
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

fn table_summary(table: MysqlTableMetadata) -> TableSummary {
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
    result: &MysqlResultSet,
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

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MysqlResultSet {
        MysqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    #[test]
    fn metadata_snapshot_uses_server_schema() {
        let snapshot = metadata_snapshot_from_result(
            "APP",
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
    fn empty_columns_sentinel_maps_to_missing_table() {
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
            vec![vec![
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
            ]],
        );

        let error = parse_columns_for_table(&result, "app", "missing").unwrap_err();
        assert!(matches!(error, DbOperationError::ObjectMissing(_)));
    }

    #[test]
    fn targeted_metadata_queries_escape_schema_and_table_literals() {
        let schema = "app\\\n\r\t\u{0008}\u{001a}'";
        let table = "items\\\n\r\t\u{0008}\u{001a}'";

        assert_eq!(
            quote_string(schema),
            format!(
                "'app{}{}{}{}{}{}{}'",
                r"\\", r"\n", r"\r", r"\t", r"\b", r"\Z", r"\'",
            )
        );
        assert_eq!(
            quote_string(table),
            format!(
                "'items{}{}{}{}{}{}{}'",
                r"\\", r"\n", r"\r", r"\t", r"\b", r"\Z", r"\'",
            )
        );

        for query in [
            table_query(schema, table),
            columns_query(schema, table),
            foreign_keys_query(schema, table),
        ] {
            assert!(query.contains(&quote_string(schema)));
            assert!(query.contains(&quote_string(table)));
        }
    }

    #[test]
    fn empty_table_list_sentinel_parses_as_no_tables() {
        let result = result(
            &[
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "TABLE_TYPE",
                "TABLE_ROWS",
                "TABLE_COMMENT",
            ],
            vec![vec![
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
            ]],
        );

        assert!(
            parse_table_metadata(&result)
                .expect("empty table metadata parses")
                .is_empty()
        );
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

        let summaries = [TableSummary::new(
            "sabiql_test".to_string(),
            "parent".to_string(),
            None,
            false,
        )];
        let foreign_keys =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), &summaries)
                .unwrap();

        assert_eq!(foreign_keys.len(), 1);
        assert_eq!(
            foreign_keys[0].from_columns,
            ["parent_first", "parent_second"]
        );
        assert_eq!(foreign_keys[0].to_columns, ["first_key", "second_key"]);
        assert_eq!(foreign_keys[0].on_update, FkAction::Cascade);
        assert_eq!(foreign_keys[0].on_delete, FkAction::SetNull);

        let unresolved =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), &[]).unwrap();
        assert!(!unresolved[0].is_reference_resolved());
    }
}
