use crate::app::ports::outbound::DbOperationError;
use crate::domain::{Column, QueryValue};

use super::super::cli::MysqlResultSet;
use super::super::sql::quote_identifier;
use super::catalog::{column_from_metadata, fetch_columns, primary_key_names};

const PREVIEW_IDENTITY_ALIAS_PREFIX: &str = "__sabiql_row_identity_";

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

pub(in crate::adapters::mysql) async fn fetch_preview_metadata(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<PreviewMetadata, DbOperationError> {
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
    let primary_key_names = primary_key_names(&column_metadata);
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
        identity_columns,
    })
}

pub(in crate::adapters::mysql) fn build_preview_query(
    schema: &str,
    table: &str,
    order_columns: &[String],
    visible_columns: &[Column],
    identity_columns: &[Column],
    limit: usize,
    offset: usize,
) -> String {
    let visible_select = visible_columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let identity_select = identity_columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            format!(
                "{} AS {}",
                quote_identifier(&column.name),
                quote_identifier(&preview_identity_alias(index)),
            )
        })
        .collect::<Vec<_>>();
    let columns = std::iter::once(visible_select)
        .chain(identity_select)
        .filter(|select| !select.is_empty())
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

pub(in crate::adapters::mysql) fn convert_preview_values(
    result: &MysqlResultSet,
    columns: &[Column],
    identity_columns: &[Column],
) -> Result<ConvertedPreviewValues, DbOperationError> {
    let expected_columns = columns
        .iter()
        .map(|column| column.name.clone())
        .chain(
            identity_columns
                .iter()
                .enumerate()
                .map(|(index, _)| preview_identity_alias(index)),
        )
        .collect::<Vec<_>>();
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
                .map(|(value, column)| Ok(convert_preview_value(value, &column.data_type)))
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

fn preview_identity_alias(index: usize) -> String {
    format!("{PREVIEW_IDENTITY_ALIAS_PREFIX}{index}")
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
    use super::*;
    use crate::domain::ColumnAttributes;

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

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0], QueryValue::Text("0x41".to_string()));
        assert_eq!(values.visible[0][1], QueryValue::Blob(vec![0, 255]));
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
    fn binary_type_recognizes_mysql_spatial_types() {
        for data_type in [
            "geometry",
            "point srid 4326",
            "linestring",
            "polygon",
            "multipoint",
            "multilinestring",
            "multipolygon",
            "geometrycollection",
        ] {
            assert!(is_binary_type(data_type), "{data_type}");
        }
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

        let query = build_preview_query(
            "app",
            "items",
            &["id".to_string()],
            &columns,
            &[],
            500,
            1000,
        );

        assert_eq!(
            query,
            "SELECT `id`, `display` FROM `app`.`items` ORDER BY `id` LIMIT 500 OFFSET 1000"
        );
    }

    #[test]
    fn preview_query_appends_aliased_hidden_primary_key_columns() {
        let visible_columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];

        let query = build_preview_query(
            "app",
            "items",
            &["id".to_string()],
            &visible_columns,
            &identity_columns,
            10,
            0,
        );

        assert_eq!(
            query,
            "SELECT `payload`, `id` AS `__sabiql_row_identity_0` FROM `app`.`items` ORDER BY `id` LIMIT 10 OFFSET 0"
        );
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
}
