use std::fmt::Write as _;

use crate::app::policy::sql::mysql_statement::{
    mysql_explain_rejection_message, mysql_tree_explain_query_kind,
};
use crate::domain::QueryValue;

pub(super) fn build_explain_sql(query: &str) -> Option<String> {
    if mysql_explain_rejection_message(query).is_some() {
        return None;
    }
    Some(format!("EXPLAIN FORMAT=TREE {query}"))
}

pub(super) fn build_explain_analyze_sql(query: &str) -> Option<String> {
    mysql_tree_explain_query_kind(&format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))?;
    Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))
}

pub(super) fn build_update_sql(
    schema: &str,
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    let where_clause = pk_pairs
        .iter()
        .map(|(column, value)| mysql_equality_predicate(column, value))
        .collect::<Vec<_>>()
        .join(" AND ");

    format!(
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        mysql_quote_identifier(schema),
        mysql_quote_identifier(table),
        mysql_quote_identifier(column),
        mysql_sql_literal(new_value),
        where_clause
    )
}

pub(super) fn build_bulk_delete_sql(
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
        .map(|pairs| {
            pairs
                .iter()
                .map(|(column, value)| mysql_equality_predicate(column, value))
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
        "DELETE FROM {}.{}\nWHERE {};",
        mysql_quote_identifier(schema),
        mysql_quote_identifier(table),
        where_clause
    )
}

pub(super) fn build_metadata_select_query(
    query: &str,
    source_alias: &str,
    marker_alias: &str,
) -> String {
    format!(
        "WITH {source_alias} AS (SELECT * FROM (({query}\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT {source_alias}.* FROM {source_alias} RIGHT JOIN (SELECT 1 AS {marker_alias}) AS __sabiql_metadata_marker ON TRUE"
    )
}

fn mysql_quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn mysql_sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => mysql_quote_string(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(hex, "{byte:02X}");
            }
            format!("X'{hex}'")
        }
    }
}

fn mysql_quote_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{001a}' => escaped.push_str("\\Z"),
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            _ => escaped.push(character),
        }
    }
    format!("'{escaped}'")
}

fn mysql_equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = mysql_quote_identifier(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", mysql_sql_literal(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_quotes_identifiers_and_mysql_string_escapes() {
        let sql = build_update_sql(
            "db`name",
            "table`name",
            "value`name",
            &QueryValue::text("O'Reilly\\path\n\t\0"),
            &[
                (
                    "id`part".to_string(),
                    QueryValue::SqlLiteral("18446744073709551615".into()),
                ),
                ("tenant".to_string(), QueryValue::Null),
            ],
        );

        assert_eq!(
            sql,
            "UPDATE \x60db\x60\x60name\x60.\x60table\x60\x60name\x60\nSET \x60value\x60\x60name\x60 = 'O\\'Reilly\\\\path\\n\\t\\0'\nWHERE \x60id\x60\x60part\x60 = 18446744073709551615 AND \x60tenant\x60 IS NULL;"
        );
    }

    #[test]
    fn update_uses_text_datetime_and_blob_literals_without_coercion() {
        let sql = build_update_sql(
            "sabiql_test",
            "events",
            "payload",
            &QueryValue::Blob(vec![0, 255, 16]),
            &[(
                "created_at".to_string(),
                QueryValue::text("2026-08-13 12:34:56"),
            )],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`events`\nSET `payload` = X'00FF10'\nWHERE `created_at` = '2026-08-13 12:34:56';"
        );
    }

    #[test]
    fn json_document_update_keeps_json_null_distinct_from_string_null() {
        let json_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text("null"),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );
        let string_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text(r#""null""#),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert!(json_null.contains("SET `payload` = 'null'"));
        assert!(string_null.contains("SET `payload` = '\"null\"'"));
        assert_ne!(json_null, string_null);
    }

    #[test]
    fn bulk_delete_targets_each_composite_primary_key_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("1".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("20".into())),
                ],
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("2".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("10".into())),
                ],
            ],
        );

        assert_eq!(
            sql,
            "DELETE FROM `sabiql_test`.`items`\nWHERE (`first` = 1 AND `second` = 20) OR (`first` = 2 AND `second` = 10);"
        );
    }

    #[test]
    fn builds_tree_explain_for_supported_queries() {
        for query in [
            "SELECT * FROM users",
            "TABLE users",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "REPLACE users VALUES (1)",
            "REPLACE LOW_PRIORITY INTO users VALUES (1)",
            "REPLACE DELAYED users VALUES (1)",
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
        ] {
            assert_eq!(
                build_explain_sql(query),
                Some(format!("EXPLAIN FORMAT=TREE {query}")),
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_tree_explain_for_unsupported_input() {
        for query in [
            "CREATE TABLE users(id INT)",
            "DROP TABLE users",
            "\\C /tmp/other.sock",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_sql(query), None, "{query}");
        }
    }

    #[test]
    fn builds_tree_explain_analyze_only_for_side_effect_free_reads() {
        for query in ["SELECT * FROM users", "TABLE users"] {
            assert_eq!(
                build_explain_analyze_sql(query),
                Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                "{query}"
            );
        }

        for query in [
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "SELECT * FROM users FOR UPDATE",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_analyze_sql(query), None, "{query}");
        }
    }

    #[test]
    fn builds_metadata_select_query_without_changing_the_fallback_sql() {
        assert_eq!(
            build_metadata_select_query("SELECT 1", "__source", "__marker"),
            "WITH __source AS (SELECT * FROM ((SELECT 1\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT __source.* FROM __source RIGHT JOIN (SELECT 1 AS __marker) AS __sabiql_metadata_marker ON TRUE"
        );
    }
}
