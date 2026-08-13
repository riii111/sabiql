use async_trait::async_trait;

use crate::app::ports::outbound::{AccessMode, DbOperationError, MetadataProvider};
use crate::domain::{
    Column, ColumnAttributes, DatabaseMetadata, QueryValue, Schema, Table, TableKind,
    TableKindInfo, TableSignature, TableSummary,
};

use super::{
    MySqlAdapter, MySqlOptionFile, MysqlResultSet, parse_mysql_dsn, run_mysql_adhoc,
    validate_mysql_values,
};

const TABLES_QUERY: &str = "SELECT TABLE_NAME, TABLE_TYPE, TABLE_ROWS FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') UNION ALL SELECT NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')) ORDER BY TABLE_NAME";

#[derive(Debug, Clone)]
struct MysqlColumnMetadata {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    comment: Option<String>,
    ordinal_position: i32,
    primary_key_position: Option<i32>,
    invisible: bool,
    generated: bool,
}

#[derive(Debug, Clone)]
pub(super) struct PreviewMetadata {
    pub visible_columns: Vec<Column>,
    pub order_columns: Vec<String>,
}

#[derive(Debug, Clone)]
struct MysqlTableMetadata {
    name: String,
    kind: TableKind,
    row_count_estimate: Option<i64>,
}

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let database = selected_database(dsn)?;
        let result = execute_metadata_query(dsn, TABLES_QUERY).await?;
        let tables = parse_table_metadata(&result)?;
        let mut metadata = DatabaseMetadata::new(database.clone());
        metadata.schemas = vec![Schema::new(database.clone())];
        metadata.table_summaries = tables
            .into_iter()
            .map(|table| table_summary(&database, table))
            .collect();
        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        fetch_table(dsn, schema, table).await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        fetch_table(dsn, schema, table).await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let metadata = self.fetch_metadata(dsn).await?;
        Ok(metadata
            .table_summaries
            .into_iter()
            .map(|summary| TableSignature {
                schema: summary.schema,
                name: summary.name,
                signature: format!("{:?}", summary.kind_info.kind),
            })
            .collect())
    }
}

pub(super) async fn fetch_preview_metadata(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<PreviewMetadata, DbOperationError> {
    find_table(dsn, schema, table).await?;
    let column_metadata = fetch_columns(dsn, schema, table).await?;
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
    let order_columns = primary_key_names(&column_metadata);
    let order_columns = if order_columns.is_empty() {
        visible_columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    } else {
        order_columns
    };
    Ok(PreviewMetadata {
        visible_columns,
        order_columns,
    })
}

pub(super) fn build_preview_query(
    schema: &str,
    table: &str,
    order_columns: &[String],
    visible_columns: &[Column],
    limit: usize,
    offset: usize,
) -> String {
    let columns = visible_columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let order_by = order_columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {columns} FROM {}.{} ORDER BY {order_by} LIMIT {limit} OFFSET {offset}",
        quote_identifier(schema),
        quote_identifier(table),
    )
}

pub(super) fn convert_preview_values(
    result: &MysqlResultSet,
    columns: &[Column],
) -> Result<Vec<Vec<QueryValue>>, DbOperationError> {
    if result.values.is_empty() {
        if result.columns.is_empty() || result.columns.len() == columns.len() {
            return Ok(Vec::new());
        }
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned an unexpected column count".to_string(),
        ));
    }
    if result.columns.len() != columns.len() {
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned an unexpected column count".to_string(),
        ));
    }
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != columns.len() {
                return Err(DbOperationError::MetadataParseFailed(
                    "MySQL preview returned an unexpected row width".to_string(),
                ));
            }
            row.iter()
                .zip(columns)
                .map(|(value, column)| Ok(convert_preview_value(value, &column.data_type)))
                .collect()
        })
        .collect()
}

fn convert_preview_value(value: &QueryValue, data_type: &str) -> QueryValue {
    let QueryValue::Text(value) = value else {
        return value.clone();
    };
    if is_binary_type(data_type)
        && let Some(bytes) = decode_hex(value)
    {
        return QueryValue::Blob(bytes);
    }
    if is_numeric_type(data_type) && is_sql_numeric_literal(value) {
        return QueryValue::SqlLiteral(value.clone());
    }
    QueryValue::Text(value.clone())
}

async fn fetch_table(dsn: &str, schema: &str, table: &str) -> Result<Table, DbOperationError> {
    let table_metadata = find_table(dsn, schema, table).await?;
    let columns = fetch_columns(dsn, schema, table).await?;
    let primary_key = primary_key_names(&columns);
    let columns = columns.iter().map(column_from_metadata).collect::<Vec<_>>();
    Ok(Table {
        schema: schema.to_string(),
        name: table_metadata.name,
        owner: None,
        columns,
        primary_key: (!primary_key.is_empty()).then_some(primary_key),
        foreign_keys: Vec::new(),
        indexes: Vec::new(),
        rls: None,
        triggers: Vec::new(),
        row_count_estimate: table_metadata.row_count_estimate,
        comment: None,
        source_ddl: None,
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            ..TableKindInfo::default()
        },
    })
}

async fn find_table(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<MysqlTableMetadata, DbOperationError> {
    let database = selected_database(dsn)?;
    if schema != database {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    let result = execute_metadata_query(dsn, TABLES_QUERY).await?;
    parse_table_metadata(&result)?
        .into_iter()
        .find(|candidate| candidate.name == table)
        .ok_or_else(|| {
            DbOperationError::ObjectMissing(format!("MySQL table not found: {schema}.{table}"))
        })
}

async fn fetch_columns(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    let query = format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = DATABASE() AND kcu.TABLE_SCHEMA = c.TABLE_SCHEMA AND kcu.TABLE_NAME = c.TABLE_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME AND kcu.CONSTRAINT_NAME = 'PRIMARY' WHERE c.TABLE_SCHEMA = DATABASE() AND c.TABLE_NAME = {} ORDER BY c.ORDINAL_POSITION",
        quote_string(table)
    );
    let result = execute_metadata_query(dsn, &query).await?;
    let database = selected_database(dsn)?;
    if schema != database {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    let columns = parse_column_metadata(&result)?;
    if columns.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL object has no column metadata: {schema}.{table}"
        )));
    }
    Ok(columns)
}

async fn execute_metadata_query(
    dsn: &str,
    query: &str,
) -> Result<MysqlResultSet, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    super::validate_mysql_tls_files(&target)?;
    let statements =
        super::validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadOnly)?;
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
    parse_mysql_dsn(dsn)?.database.ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "MySQL metadata requires a selected database".to_string(),
        )
    })
}

fn parse_table_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlTableMetadata>, DbOperationError> {
    expect_columns(result, &["TABLE_NAME", "TABLE_TYPE", "TABLE_ROWS"])?;
    let tables = result
        .values
        .iter()
        .map(|row| {
            if row.len() != 3 {
                return Err(metadata_shape_error("TABLES row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            let name = required_text(&row[0], "TABLE_NAME")?.to_string();
            let kind = match required_text(&row[1], "TABLE_TYPE")? {
                "BASE TABLE" => TableKind::Table,
                "VIEW" => TableKind::View,
                value => {
                    return Err(DbOperationError::MetadataParseFailed(format!(
                        "unexpected MySQL table type: {value}"
                    )));
                }
            };
            let row_count_estimate = optional_text(&row[2], "TABLE_ROWS")?
                .map(|value| {
                    value.parse::<i64>().map_err(|_| {
                        DbOperationError::MetadataParseFailed(
                            "invalid MySQL TABLE_ROWS value".to_string(),
                        )
                    })
                })
                .transpose()?;
            Ok(Some(MysqlTableMetadata {
                name,
                kind,
                row_count_estimate,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tables.into_iter().flatten().collect())
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
        })
        .collect()
}

fn column_from_metadata(metadata: &MysqlColumnMetadata) -> Column {
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

fn primary_key_names(columns: &[MysqlColumnMetadata]) -> Vec<String> {
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

fn table_summary(database: &str, table: MysqlTableMetadata) -> TableSummary {
    TableSummary::new(
        database.to_string(),
        table.name,
        table.row_count_estimate,
        false,
    )
    .with_kind_info(TableKindInfo {
        kind: table.kind,
        ..TableKindInfo::default()
    })
}

fn expect_columns(result: &MysqlResultSet, expected: &[&str]) -> Result<(), DbOperationError> {
    if result.columns == expected {
        Ok(())
    } else {
        Err(metadata_shape_error("MySQL metadata columns"))
    }
}

fn required_text<'a>(value: &'a QueryValue, field: &str) -> Result<&'a str, DbOperationError> {
    optional_text(value, field)?.ok_or_else(|| {
        DbOperationError::MetadataParseFailed(format!("MySQL metadata field is NULL: {field}"))
    })
}

fn optional_text<'a>(
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

fn parse_positive_i32(value: &QueryValue, field: &str) -> Result<i32, DbOperationError> {
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

fn metadata_shape_error(field: &str) -> DbOperationError {
    DbOperationError::MetadataParseFailed(format!("unexpected MySQL metadata shape: {field}"))
}

fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn quote_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn is_binary_type(data_type: &str) -> bool {
    let name = data_type
        .split(['(', ' '])
        .next()
        .unwrap_or(data_type)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "binary" | "varbinary" | "tinyblob" | "blob" | "mediumblob" | "longblob" | "bit"
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
    use super::*;

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MysqlResultSet {
        MysqlResultSet {
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
        }
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

        let values = convert_preview_values(&result, &columns).expect("conversion succeeds");

        assert_eq!(values[0][0], QueryValue::Text("0x41".to_string()));
        assert_eq!(values[0][1], QueryValue::Blob(vec![0, 255]));
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

        let values = convert_preview_values(&result, &columns).expect("conversion succeeds");

        assert_eq!(values[0][0].as_str(), Some("18446744073709551615"));
        assert!(matches!(values[0][0], QueryValue::SqlLiteral(_)));
        assert!(matches!(values[0][1], QueryValue::SqlLiteral(_)));
        assert!(matches!(values[0][2], QueryValue::SqlLiteral(_)));
    }

    #[test]
    fn numeric_literal_validation_rejects_partial_values() {
        assert!(is_sql_numeric_literal("1."));
        assert!(is_sql_numeric_literal(".5"));
        assert!(is_sql_numeric_literal("-1.2e-3"));
        assert!(!is_sql_numeric_literal("1e"));
        assert!(!is_sql_numeric_literal("nan"));
    }

    #[test]
    fn preview_query_lists_visible_columns_and_orders_by_primary_key() {
        let columns = vec![column("id", "int"), column("display", "text")];

        let query = build_preview_query("app", "items", &["id".to_string()], &columns, 500, 1000);

        assert_eq!(
            query,
            "SELECT `id`, `display` FROM `app`.`items` ORDER BY `id` LIMIT 500 OFFSET 1000"
        );
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
    fn empty_table_list_sentinel_parses_as_no_tables() {
        let result = result(
            &["TABLE_NAME", "TABLE_TYPE", "TABLE_ROWS"],
            vec![vec![QueryValue::Null, QueryValue::Null, QueryValue::Null]],
        );

        assert!(
            parse_table_metadata(&result)
                .expect("empty table metadata parses")
                .is_empty()
        );
    }
}
