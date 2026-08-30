use crate::domain::{DatabaseType, QueryValue, mysql_sql, postgres_sql, sqlite_sql};

pub fn build_explain_sql(database_type: DatabaseType, query: &str) -> Option<String> {
    match database_type {
        DatabaseType::PostgreSQL => postgres_sql::build_explain_sql(query),
        DatabaseType::MySQL => mysql_sql::build_explain_sql(query),
        DatabaseType::SQLite => sqlite_sql::build_sqlite_explain_query_plan_sql(query),
    }
}

pub fn build_explain_analyze_sql(database_type: DatabaseType, query: &str) -> Option<String> {
    match database_type {
        DatabaseType::PostgreSQL => postgres_sql::build_explain_analyze_sql(query),
        DatabaseType::MySQL => mysql_sql::build_explain_analyze_sql(query),
        DatabaseType::SQLite => None,
    }
}

pub fn build_update_sql(
    database_type: DatabaseType,
    schema: &str,
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    match database_type {
        DatabaseType::PostgreSQL => {
            postgres_sql::build_update_sql(schema, table, column, new_value, pk_pairs)
        }
        DatabaseType::MySQL => {
            mysql_sql::build_update_sql(schema, table, column, new_value, pk_pairs)
        }
        DatabaseType::SQLite => sqlite_sql::build_update_sql(table, column, new_value, pk_pairs),
    }
}

pub fn build_bulk_delete_sql(
    database_type: DatabaseType,
    schema: &str,
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    match database_type {
        DatabaseType::PostgreSQL => {
            postgres_sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row)
        }
        DatabaseType::MySQL => mysql_sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row),
        DatabaseType::SQLite => sqlite_sql::build_bulk_delete_sql(table, pk_pairs_per_row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_explain_sql_to_each_database() {
        assert_eq!(
            build_explain_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
    }

    #[test]
    fn dispatches_explain_analyze_sql_to_supported_databases() {
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
            None
        );
    }

    #[test]
    fn dispatches_update_sql_with_database_specific_output() {
        let pairs = [("id".to_string(), QueryValue::Null)];

        assert_eq!(
            build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text("Ada"),
                &pairs,
            ),
            "UPDATE \"public\".\"users\"\nSET \"name\" = 'Ada'\nWHERE \"id\" IS NULL;"
        );
        assert_eq!(
            build_update_sql(
                DatabaseType::MySQL,
                "sabiql_test",
                "users",
                "name",
                &QueryValue::text("Ada"),
                &pairs,
            ),
            "UPDATE `sabiql_test`.`users`\nSET `name` = 'Ada'\nWHERE `id` IS NULL;"
        );
        assert_eq!(
            build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &pairs,
            ),
            "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" IS NULL;"
        );
    }

    #[test]
    fn dispatches_bulk_delete_sql_with_database_specific_output() {
        let rows = vec![
            vec![("id".to_string(), QueryValue::SqlLiteral("1".into()))],
            vec![("id".to_string(), QueryValue::SqlLiteral("2".into()))],
        ];

        assert_eq!(
            build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows),
            "DELETE FROM \"public\".\"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
        assert_eq!(
            build_bulk_delete_sql(DatabaseType::MySQL, "sabiql_test", "users", &rows),
            "DELETE FROM `sabiql_test`.`users`\nWHERE (`id` = 1) OR (`id` = 2);"
        );
        assert_eq!(
            build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows),
            "DELETE FROM \"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
    }
}
