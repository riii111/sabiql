use crate::app::ports::SqlDialect;
use crate::infra::utils::{quote_ident, quote_literal};

use super::super::PostgresAdapter;

fn sql_literal_or_null(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        quote_literal(value)
    }
}

impl SqlDialect for PostgresAdapter {
    fn build_update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| format!("{} = {}", quote_ident(col), quote_literal(val)))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
            quote_ident(schema),
            quote_ident(table),
            quote_ident(column),
            sql_literal_or_null(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let pk_count = pk_pairs_per_row[0].len();

        let where_clause = if pk_count == 1 {
            let col = quote_ident(&pk_pairs_per_row[0][0].0);
            let values = pk_pairs_per_row
                .iter()
                .map(|pairs| sql_literal_or_null(&pairs[0].1))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} IN ({})", col, values)
        } else {
            let cols = pk_pairs_per_row[0]
                .iter()
                .map(|(col, _)| quote_ident(col))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = pk_pairs_per_row
                .iter()
                .map(|pairs| {
                    let vals = pairs
                        .iter()
                        .map(|(_, val)| sql_literal_or_null(val))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", vals)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) IN ({})", cols, rows)
        };

        format!(
            "DELETE FROM {}.{}\nWHERE {};",
            quote_ident(schema),
            quote_ident(table),
            where_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ports::SqlDialect;
    use crate::infra::adapters::postgres::PostgresAdapter;

    mod sql_dialect_update {
        use super::*;

        #[test]
        fn single_pk_returns_escaped_sql() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "name",
                "O'Reilly",
                &[("id".into(), "42".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "s",
                "t",
                "name",
                "new",
                &[("id".into(), "1".into()), ("tenant_id".into(), "7".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }
    }

    mod sql_dialect_update_edge_cases {
        use super::*;

        #[test]
        fn null_value_generates_unquoted_null() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "name",
                "NULL",
                &[("id".into(), "1".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn empty_string_value_generates_empty_literal() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "name",
                "",
                &[("id".into(), "1".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = ''\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn column_name_with_double_quote_is_escaped() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "my\"col",
                "val",
                &[("id".into(), "1".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"my\"\"col\" = 'val'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn backslash_in_value_is_preserved_as_literal() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "path",
                "C:\\Users\\test",
                &[("id".into(), "1".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"path\" = 'C:\\Users\\test'\nWHERE \"id\" = '1';"
            );
        }
    }

    mod sql_dialect_bulk_delete {
        use super::*;

        #[test]
        fn single_pk_single_row_returns_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "1".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1');"
            );
        }

        #[test]
        fn single_pk_multiple_rows_returns_in_clause_with_all_values() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![("id".to_string(), "1".to_string())],
                vec![("id".to_string(), "2".to_string())],
                vec![("id".to_string(), "3".to_string())],
            ];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1', '2', '3');"
            );
        }

        #[test]
        fn composite_pk_returns_row_constructor_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), "1".to_string()),
                    ("tenant_id".to_string(), "a".to_string()),
                ],
                vec![
                    ("id".to_string(), "2".to_string()),
                    ("tenant_id".to_string(), "b".to_string()),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql("s", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"s\".\"t\"\nWHERE (\"id\", \"tenant_id\") IN (('1', 'a'), ('2', 'b'));"
            );
        }

        #[test]
        fn null_pk_value_uses_null_literal() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "NULL".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN (NULL);");
        }

        #[test]
        fn pk_value_with_quotes_is_escaped() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "O'Reilly".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN ('O''Reilly');"
            );
        }

        #[test]
        fn empty_string_pk_value_returns_empty_literal() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), String::new())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN ('');");
        }

        #[test]
        fn column_name_with_double_quote_is_escaped() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("my\"pk".to_string(), "1".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"my\"\"pk\" IN ('1');"
            );
        }
    }

    mod sql_literal_or_null_tests {
        use super::super::sql_literal_or_null;
        use rstest::rstest;

        #[rstest]
        #[case("NULL", "NULL")]
        #[case("null", "'null'")]
        #[case("", "''")]
        #[case("hello", "'hello'")]
        #[case("it's", "'it''s'")]
        #[case("NULL ", "'NULL '")]
        fn value_expr_returns_expected(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(sql_literal_or_null(input), expected);
        }
    }
}
