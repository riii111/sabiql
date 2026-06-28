#[cfg(any(test, feature = "test-support"))]
use std::fmt::Write as _;
use std::sync::Arc;

#[cfg(any(test, feature = "test-support"))]
use crate::domain::{DatabaseType, QueryValue, Table};

use super::ports::outbound::{DdlGenerator, SqlDialect};
pub struct AppServices {
    pub ddl_generator: Arc<dyn DdlGenerator>,
    pub sql_dialect: Arc<dyn SqlDialect>,
}

impl AppServices {
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn stub() -> Self {
        use crate::policy::sql::sqlite_explain::build_sqlite_explain_query_plan_sql;

        fn quote_literal(value: &str) -> String {
            format!("'{}'", value.replace('\'', "''"))
        }

        fn sql_literal(database_type: DatabaseType, value: &QueryValue) -> String {
            match value {
                QueryValue::Null => "NULL".to_string(),
                QueryValue::Text(value) => quote_literal(value),
                QueryValue::SqlLiteral(value) => value.clone(),
                QueryValue::Blob(bytes) => {
                    let mut hex = String::with_capacity(bytes.len() * 2);
                    for byte in bytes {
                        let _ = write!(hex, "{byte:02x}");
                    }
                    match database_type {
                        DatabaseType::PostgreSQL => format!("'\\x{hex}'"),
                        DatabaseType::SQLite => format!("X'{}'", hex.to_uppercase()),
                    }
                }
            }
        }

        fn equality_predicate(
            database_type: DatabaseType,
            key: &str,
            value: &QueryValue,
        ) -> String {
            match value {
                QueryValue::Null => format!("\"{key}\" IS NULL"),
                _ => format!("\"{key}\" = {}", sql_literal(database_type, value)),
            }
        }

        struct StubDdlGenerator;
        impl DdlGenerator for StubDdlGenerator {
            fn generate_ddl(&self, _database_type: DatabaseType, _table: &Table) -> String {
                unimplemented!("inject a real DdlGenerator via AppServices")
            }
            fn ddl_line_count(&self, _database_type: DatabaseType, _table: &Table) -> usize {
                0
            }
        }

        struct StubSqlDialect;
        impl SqlDialect for StubSqlDialect {
            fn build_explain_sql(
                &self,
                database_type: DatabaseType,
                query: &str,
            ) -> Option<String> {
                match database_type {
                    DatabaseType::PostgreSQL => Some(format!("EXPLAIN {query}")),
                    DatabaseType::SQLite => build_sqlite_explain_query_plan_sql(query),
                }
            }

            fn build_explain_analyze_sql(
                &self,
                database_type: DatabaseType,
                query: &str,
            ) -> Option<String> {
                match database_type {
                    DatabaseType::PostgreSQL => Some(format!("EXPLAIN ANALYZE {query}")),
                    DatabaseType::SQLite => None,
                }
            }

            fn build_update_sql(
                &self,
                database_type: DatabaseType,
                schema: &str,
                table: &str,
                column: &str,
                new_value: &QueryValue,
                pk_pairs: &[(String, QueryValue)],
            ) -> String {
                let set_clause =
                    format!("\"{column}\" = {}", sql_literal(database_type, new_value));
                let where_clause = pk_pairs
                    .iter()
                    .map(|(key, value)| equality_predicate(database_type, key, value))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                match database_type {
                    DatabaseType::PostgreSQL => {
                        format!(
                            "UPDATE \"{schema}\".\"{table}\" SET {set_clause} WHERE {where_clause}"
                        )
                    }
                    DatabaseType::SQLite => {
                        format!("UPDATE \"{table}\" SET {set_clause} WHERE {where_clause}")
                    }
                }
            }
            fn build_bulk_delete_sql(
                &self,
                database_type: DatabaseType,
                schema: &str,
                table: &str,
                pk_pairs_per_row: &[Vec<(String, QueryValue)>],
            ) -> String {
                let where_clause = pk_pairs_per_row
                    .iter()
                    .map(|pk_pairs| {
                        pk_pairs
                            .iter()
                            .map(|(key, value)| equality_predicate(database_type, key, value))
                            .collect::<Vec<_>>()
                            .join(" AND ")
                    })
                    .collect::<Vec<_>>()
                    .join(" OR ");
                match database_type {
                    DatabaseType::PostgreSQL => {
                        format!("DELETE FROM \"{schema}\".\"{table}\" WHERE {where_clause}")
                    }
                    DatabaseType::SQLite => format!("DELETE FROM \"{table}\" WHERE {where_clause}"),
                }
            }
        }

        Self {
            ddl_generator: Arc::new(StubDdlGenerator),
            sql_dialect: Arc::new(StubSqlDialect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DatabaseType, QueryValue};

    #[test]
    fn stub_sqlite_explain_passes_through_existing_query_plan_prefix() {
        let services = AppServices::stub();

        assert_eq!(
            services
                .sql_dialect
                .build_explain_sql(DatabaseType::SQLite, "EXPLAIN QUERY PLAN SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
    }

    #[test]
    fn stub_sql_dialect_formats_typed_values() {
        let services = AppServices::stub();

        let update = services.sql_dialect.build_update_sql(
            DatabaseType::PostgreSQL,
            "public",
            "files",
            "payload",
            &QueryValue::Blob(vec![0, 255]),
            &[("deleted_at".to_string(), QueryValue::Null)],
        );
        let delete = services.sql_dialect.build_bulk_delete_sql(
            DatabaseType::SQLite,
            "main",
            "files",
            &[vec![(
                "payload".to_string(),
                QueryValue::Blob(vec![0, 255]),
            )]],
        );

        assert_eq!(
            update,
            "UPDATE \"public\".\"files\" SET \"payload\" = '\\x00ff' WHERE \"deleted_at\" IS NULL"
        );
        assert_eq!(delete, "DELETE FROM \"files\" WHERE \"payload\" = X'00FF'");
    }
}
