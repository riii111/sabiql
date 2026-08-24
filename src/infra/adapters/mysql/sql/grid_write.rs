use crate::domain::QueryValue;

use crate::adapters::bulk_delete::rows_predicate;

use super::literal::{equality_predicate, quote_identifier, sql_literal};

pub(super) fn build_update_sql(
    schema: &str,
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
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        quote_identifier(schema),
        quote_identifier(table),
        quote_identifier(column),
        sql_literal(new_value),
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

    let where_clause = rows_predicate(pk_pairs_per_row, equality_predicate);

    format!(
        "DELETE FROM {}.{}\nWHERE {};",
        quote_identifier(schema),
        quote_identifier(table),
        where_clause
    )
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
    fn update_uses_null_literal_for_cell_value() {
        let sql = build_update_sql(
            "sabiql_test",
            "items",
            "payload",
            &QueryValue::Null,
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`items`\nSET `payload` = NULL\nWHERE `id` = 1;"
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
    fn bulk_delete_uses_unwrapped_predicate_for_single_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[vec![("id".to_string(), QueryValue::SqlLiteral("1".into()))]],
        );

        assert_eq!(sql, "DELETE FROM `sabiql_test`.`items`\nWHERE `id` = 1;");
    }
}
