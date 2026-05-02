use std::sync::Arc;

#[cfg(any(test, feature = "test-support"))]
use crate::domain::DatabaseType;

use super::ports::outbound::{DdlGenerator, SqlDialect};

pub struct AppServices {
    pub ddl_generator: Arc<dyn DdlGenerator>,
    pub sql_dialect: Arc<dyn SqlDialect>,
}

impl AppServices {
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn stub() -> Self {
        struct StubDdlGenerator;
        impl DdlGenerator for StubDdlGenerator {
            fn generate_ddl(
                &self,
                _database_type: DatabaseType,
                _table: &crate::domain::Table,
            ) -> String {
                unimplemented!("inject a real DdlGenerator via AppServices")
            }
            fn ddl_line_count(
                &self,
                _database_type: DatabaseType,
                _table: &crate::domain::Table,
            ) -> usize {
                0
            }
        }

        struct StubSqlDialect;
        impl SqlDialect for StubSqlDialect {
            fn build_explain_sql(&self, query: &str) -> Option<String> {
                Some(format!("EXPLAIN {query}"))
            }

            fn build_explain_analyze_sql(&self, query: &str) -> Option<String> {
                Some(format!("EXPLAIN ANALYZE {query}"))
            }

            fn build_update_sql(
                &self,
                database_type: DatabaseType,
                schema: &str,
                table: &str,
                column: &str,
                new_value: &str,
                pk_pairs: &[(String, String)],
            ) -> String {
                let set_clause = format!("\"{column}\" = '{new_value}'");
                let where_clause = pk_pairs
                    .iter()
                    .map(|(key, value)| format!("\"{key}\" = '{value}'"))
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
                pk_pairs_per_row: &[Vec<(String, String)>],
            ) -> String {
                let where_clause = pk_pairs_per_row
                    .iter()
                    .map(|pk_pairs| {
                        pk_pairs
                            .iter()
                            .map(|(key, value)| format!("\"{key}\" = '{value}'"))
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
