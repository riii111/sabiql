use std::fmt::Write as _;

use crate::QueryValue;

pub fn build_explain_sql(query: &str) -> Option<String> {
    Some(format!("EXPLAIN {query}"))
}

pub fn build_explain_analyze_sql(query: &str) -> Option<String> {
    Some(format!("EXPLAIN ANALYZE {query}"))
}

pub fn build_update_sql(
    schema: &str,
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    let where_clause = row_predicate(pk_pairs);

    format!(
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        quote_ident(schema),
        quote_ident(table),
        quote_ident(column),
        sql_literal(new_value),
        where_clause
    )
}

pub fn build_bulk_delete_sql(
    schema: &str,
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    assert!(
        !pk_pairs_per_row.is_empty(),
        "pk_pairs_per_row must not be empty"
    );

    let predicates = pk_pairs_per_row
        .iter()
        .map(|pk_pairs| row_predicate(pk_pairs))
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
        "DELETE FROM {}.{}\nWHERE {};",
        quote_ident(schema),
        quote_ident(table),
        where_clause
    )
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => quote_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(hex, "{byte:02x}");
            }
            format!("'\\x{hex}'")
        }
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

fn row_predicate(pk_pairs: &[(String, QueryValue)]) -> String {
    pk_pairs
        .iter()
        .map(|(column, value)| equality_predicate(column, value))
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
#[path = "postgres_sql_tests.rs"]
mod tests;
