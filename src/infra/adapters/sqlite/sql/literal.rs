use std::fmt::Write as _;

use crate::domain::QueryValue;

const SQLITE_NUL_TEXT_TRANSPORT_TAG: &str = "SABIQL_HEX:";

pub(in crate::adapters::sqlite) const PREVIEW_TRANSPORT_UNISTR_PREFIX: &str = "\\u0001SABIQL_HEX:";
pub(in crate::adapters::sqlite) fn sqlite_nul_text_sentinel() -> String {
    format!("\x01{SQLITE_NUL_TEXT_TRANSPORT_TAG}")
}

pub(super) fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => text_sql_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => blob_sql_literal(bytes),
    }
}

pub(super) fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

pub(super) fn rows_predicate(pk_pairs_per_row: &[Vec<(String, QueryValue)>]) -> String {
    let predicates = pk_pairs_per_row
        .iter()
        .map(|pairs| row_predicate(pairs))
        .collect::<Vec<_>>();
    if predicates.len() == 1 {
        predicates[0].clone()
    } else {
        predicates
            .into_iter()
            .map(|predicate| format!("({predicate})"))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}

pub(super) fn encode_preview_column_expr(column: &str) -> String {
    let ident = quote_ident(column);
    format!(
        "CASE WHEN typeof({ident}) = 'text' \
         THEN char(1) || '{SQLITE_NUL_TEXT_TRANSPORT_TAG}' || hex({ident}) \
         ELSE {ident} END AS {ident}"
    )
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
        quote_literal(value)
    }
}

fn row_predicate(pk_pairs: &[(String, QueryValue)]) -> String {
    pk_pairs
        .iter()
        .map(|(col, val)| equality_predicate(col, val))
        .collect::<Vec<_>>()
        .join(" AND ")
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

    mod quoting {
        use super::*;

        #[test]
        fn identifier_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
        }

        #[test]
        fn literal_escapes_embedded_quotes() {
            assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        }

        #[test]
        fn sql_literal_preserves_typed_values() {
            assert_eq!(sql_literal(&QueryValue::Null), "NULL");
            assert_eq!(sql_literal(&QueryValue::text("NULL")), "'NULL'");
            assert_eq!(sql_literal(&QueryValue::text("null")), "'null'");
            assert_eq!(sql_literal(&QueryValue::Blob(vec![0, 255])), "X'00FF'");
            assert_eq!(sql_literal(&QueryValue::SqlLiteral("42".to_string())), "42");
            assert_eq!(
                sql_literal(&QueryValue::SqlLiteral("1e999".to_string())),
                "1e999"
            );
        }
    }

    mod text_literal_encoding {
        use super::*;

        #[test]
        fn uses_cast_for_embedded_nul_byte() {
            assert_eq!(
                sql_literal(&QueryValue::text("a\0bc")),
                "CAST(X'61006263' AS TEXT)"
            );
        }
    }
}
