use crate::app::policy::sql::sqlite_explain::build_sqlite_explain_query_plan_sql;
use crate::app::ports::outbound::SqlDialect;
use crate::domain::{DatabaseType, QueryValue};

use super::super::SqliteAdapter;
use super::literal::{equality_predicate, quote_ident, rows_predicate, sql_literal};

impl SqlDialect for SqliteAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        build_sqlite_explain_query_plan_sql(query)
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        _query: &str,
    ) -> Option<String> {
        None
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
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
            "UPDATE {}\nSET {} = {}\nWHERE {};",
            quote_ident(table),
            quote_ident(column),
            sql_literal(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let where_clause = rows_predicate(pk_pairs_per_row);

        format!(
            "DELETE FROM {}\nWHERE {};",
            quote_ident(table),
            where_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod explain_queries {
        use super::*;

        #[test]
        fn wraps_select_with_query_plan() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
                Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "WITH cte AS (SELECT 1 AS n) SELECT * FROM cte"
                ),
                Some(
                    "EXPLAIN QUERY PLAN WITH cte AS (SELECT 1 AS n) SELECT * FROM cte".to_string()
                )
            );
        }

        #[test]
        fn wraps_dml_with_query_plan() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "DELETE FROM users"),
                Some("EXPLAIN QUERY PLAN DELETE FROM users".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "UPDATE users SET name = 'a' WHERE id = 1"
                ),
                Some("EXPLAIN QUERY PLAN UPDATE users SET name = 'a' WHERE id = 1".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "INSERT INTO users(name) SELECT name FROM old_users"
                ),
                Some(
                    "EXPLAIN QUERY PLAN INSERT INTO users(name) SELECT name FROM old_users"
                        .to_string()
                )
            );
            assert_eq!(
                adapter
                    .build_explain_sql(DatabaseType::SQLite, "REPLACE INTO users(id) VALUES (1)"),
                Some("EXPLAIN QUERY PLAN REPLACE INTO users(id) VALUES (1)".to_string())
            );
        }

        #[test]
        fn rejects_prefixed_explain_and_analyze() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "EXPLAIN SELECT 1"),
                None
            );
            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "CREATE TABLE users(id INTEGER)"),
                None
            );
            assert_eq!(
                adapter.build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
                None
            );
        }

        #[test]
        fn passes_through_existing_query_plan_prefix() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "EXPLAIN QUERY PLAN SELECT * FROM users"
                ),
                Some("EXPLAIN QUERY PLAN SELECT * FROM users".to_string())
            );
        }
    }

    mod update_sql {
        use super::*;

        #[test]
        fn single_pk_omits_schema_and_escapes_sql() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("O'Reilly"),
                &[("id".into(), QueryValue::text("42"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("new"),
                &[
                    ("id".into(), QueryValue::text("1")),
                    ("tenant_id".into(), QueryValue::text("7")),
                ],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }

        #[test]
        fn null_value_generates_unquoted_null() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::Null,
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn text_null_value_generates_quoted_text() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("NULL"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'NULL'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn nul_text_value_uses_cast_literal() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" = '1';"
            );
        }
    }

    mod bulk_delete_sql {
        use super::*;

        #[test]
        fn single_pk_multiple_rows_returns_or_predicates() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![("id".to_string(), QueryValue::text("1"))],
                vec![("id".to_string(), QueryValue::text("2"))],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1') OR (\"id\" = '2');"
            );
        }

        #[test]
        fn composite_pk_returns_or_predicates() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), QueryValue::text("1")),
                    ("tenant_id".to_string(), QueryValue::text("10")),
                ],
                vec![
                    ("id".to_string(), QueryValue::text("2")),
                    ("tenant_id".to_string(), QueryValue::text("20")),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1' AND \"tenant_id\" = '10') OR (\"id\" = '2' AND \"tenant_id\" = '20');"
            );
        }

        #[test]
        fn update_null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" IS NULL;"
            );
        }

        #[test]
        fn null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" IS NULL;");
        }

        #[test]
        fn composite_null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![
                ("id".to_string(), QueryValue::Null),
                ("tenant_id".to_string(), QueryValue::text("10")),
            ]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" IS NULL AND \"tenant_id\" = '10';"
            );
        }

        #[test]
        fn blob_pk_value_uses_blob_literal() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Blob(vec![0, 255, 65]))]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" = X'00FF41';");
        }

        #[test]
        fn nul_text_pk_value_uses_cast_literal() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![(
                "id".to_string(),
                QueryValue::Text("a\0bc".to_string()),
            )]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" = CAST(X'61006263' AS TEXT);"
            );
        }
    }
}
