use std::fmt::Write as _;

use crate::app::ports::outbound::SqlDialect;
use crate::domain::{DatabaseType, QueryValue};

use super::super::PostgresAdapter;
use super::{quote_ident, quote_literal};

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
        .map(|(col, val)| equality_predicate(col, val))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn rows_predicate(pk_pairs_per_row: &[Vec<(String, QueryValue)>]) -> String {
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

impl SqlDialect for PostgresAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        Some(format!("EXPLAIN {query}"))
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        Some(format!("EXPLAIN ANALYZE {query}"))
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| equality_predicate(col, val))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
            quote_ident(schema),
            quote_ident(table),
            quote_ident(column),
            sql_literal(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let where_clause = rows_predicate(pk_pairs_per_row);

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
    use crate::adapters::postgres::PostgresAdapter;
    use crate::app::ports::outbound::SqlDialect;
    use crate::domain::{DatabaseType, QueryValue};

    mod sql_dialect_update {
        use super::*;

        #[test]
        fn single_pk_returns_escaped_sql() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text("O'Reilly"),
                &[("id".into(), QueryValue::text("42"))],
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
                DatabaseType::PostgreSQL,
                "s",
                "t",
                "name",
                &QueryValue::text("new"),
                &[
                    ("id".into(), QueryValue::text("1")),
                    ("tenant_id".into(), QueryValue::text("7")),
                ],
            );

            assert_eq!(
                sql,
                "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }
    }

    mod sql_dialect_explain {
        use super::*;

        #[test]
        fn explain_sql_uses_postgres_prefix() {
            let adapter = PostgresAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::PostgreSQL, "SELECT 1"),
                Some("EXPLAIN SELECT 1".to_string())
            );
        }

        #[test]
        fn explain_analyze_sql_uses_postgres_prefix() {
            let adapter = PostgresAdapter::new();

            assert_eq!(
                adapter.build_explain_analyze_sql(DatabaseType::PostgreSQL, "SELECT 1"),
                Some("EXPLAIN ANALYZE SELECT 1".to_string())
            );
        }
    }

    mod sql_dialect_update_edge_cases {
        use super::*;

        #[test]
        fn null_value_generates_unquoted_null() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::Null,
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn text_null_value_generates_quoted_text() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text("NULL"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'NULL'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn empty_string_value_generates_empty_literal() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text(""),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = ''\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn build_update_sql_escapes_column_name() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "my\"col",
                &QueryValue::text("val"),
                &[("id".into(), QueryValue::text("1"))],
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
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "path",
                &QueryValue::text("C:\\Users\\test"),
                &[("id".into(), QueryValue::text("1"))],
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
        fn single_pk_single_row_returns_predicate() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::text("1"))]];

            let sql =
                adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '1';");
        }

        #[test]
        fn single_pk_multiple_rows_returns_or_predicates() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![("id".to_string(), QueryValue::text("1"))],
                vec![("id".to_string(), QueryValue::text("2"))],
                vec![("id".to_string(), QueryValue::text("3"))],
            ];

            let sql =
                adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE (\"id\" = '1') OR (\"id\" = '2') OR (\"id\" = '3');"
            );
        }

        #[test]
        fn composite_pk_returns_or_predicates() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), QueryValue::text("1")),
                    ("tenant_id".to_string(), QueryValue::text("a")),
                ],
                vec![
                    ("id".to_string(), QueryValue::text("2")),
                    ("tenant_id".to_string(), QueryValue::text("b")),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "s", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"s\".\"t\"\nWHERE (\"id\" = '1' AND \"tenant_id\" = 'a') OR (\"id\" = '2' AND \"tenant_id\" = 'b');"
            );
        }

        #[test]
        fn null_pk_value_uses_is_null_predicate() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IS NULL;");
        }

        #[test]
        fn update_null_pk_value_uses_is_null_predicate() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'new'\nWHERE \"id\" IS NULL;"
            );
        }

        #[test]
        fn pk_value_with_quotes_is_escaped() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::text("O'Reilly"))]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"id\" = 'O''Reilly';"
            );
        }

        #[test]
        fn empty_string_pk_value_returns_empty_literal() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::text(""))]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" = '';");
        }

        #[test]
        fn build_bulk_delete_sql_escapes_column_name() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("my\"pk".to_string(), QueryValue::text("1"))]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"my\"\"pk\" = '1';"
            );
        }
    }

    mod sql_literal_tests {
        use super::super::sql_literal;
        use crate::domain::QueryValue;
        use rstest::rstest;

        #[rstest]
        #[case("NULL", "'NULL'")]
        #[case("null", "'null'")]
        #[case("", "''")]
        #[case("hello", "'hello'")]
        #[case("it's", "'it''s'")]
        #[case("NULL ", "'NULL '")]
        fn formats_sql_literal(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(sql_literal(&QueryValue::text(input)), expected);
        }

        #[test]
        fn formats_non_text_query_values() {
            assert_eq!(sql_literal(&QueryValue::Null), "NULL");
            assert_eq!(
                sql_literal(&QueryValue::Blob(vec![0, 255, 65])),
                "'\\x00ff41'"
            );
            assert_eq!(sql_literal(&QueryValue::SqlLiteral("42".to_string())), "42");
        }
    }
}
