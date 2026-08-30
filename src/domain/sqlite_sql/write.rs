use std::fmt::Write as _;

use crate::QueryValue;

pub fn build_update_sql(
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    let where_clause = pk_pairs
        .iter()
        .map(|(column, value)| equality_predicate(column, value))
        .collect::<Vec<_>>()
        .join(" AND ");

    format!(
        "UPDATE {}\nSET {} = {}\nWHERE {};",
        quote_ident(table),
        quote_ident(column),
        sql_literal(new_value),
        where_clause
    )
}

pub fn build_bulk_delete_sql(
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    assert!(
        !pk_pairs_per_row.is_empty(),
        "pk_pairs_per_row must not be empty"
    );

    let predicates = pk_pairs_per_row
        .iter()
        .map(|pairs| {
            pairs
                .iter()
                .map(|(column, value)| equality_predicate(column, value))
                .collect::<Vec<_>>()
                .join(" AND ")
        })
        .collect::<Vec<_>>();
    let where_clause = if predicates.len() == 1 {
        predicates[0].clone()
    } else {
        predicates
            .into_iter()
            .map(|predicate| format!("({predicate})"))
            .collect::<Vec<_>>()
            .join(" OR ")
    };

    format!(
        "DELETE FROM {}\nWHERE {};",
        quote_ident(table),
        where_clause
    )
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => text_sql_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => blob_sql_literal(bytes),
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

fn blob_sql_literal(bytes: &[u8]) -> String {
    format!("X'{}'", encode_bytes_as_sql_hex(bytes))
}

fn text_sql_literal(value: &str) -> String {
    if value.contains('\0') {
        format!(
            "CAST(X'{}' AS TEXT)",
            encode_bytes_as_sql_hex(value.as_bytes())
        )
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

fn encode_bytes_as_sql_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02X}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_update_sql_without_schema_and_preserves_nul_text() {
        assert_eq!(
            build_update_sql(
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &[("id".into(), QueryValue::text("1"))],
            ),
            "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" = '1';"
        );
    }

    #[test]
    fn builds_bulk_delete_sql_with_sqlite_predicates() {
        assert_eq!(
            build_bulk_delete_sql(
                "users",
                &[
                    vec![("id".into(), QueryValue::Null)],
                    vec![("id".into(), QueryValue::Blob(vec![0, 255]))],
                ],
            ),
            "DELETE FROM \"users\"\nWHERE (\"id\" IS NULL) OR (\"id\" = X'00FF');"
        );
    }
}

#[cfg(test)]
#[path = "write_legacy_tests.rs"]
mod legacy_tests;
