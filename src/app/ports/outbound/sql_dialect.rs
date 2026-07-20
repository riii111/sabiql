use crate::domain::{DatabaseType, QueryValue};

pub trait SqlDialect: Send + Sync {
    fn build_explain_sql(&self, database_type: DatabaseType, query: &str) -> Option<String>;
    fn build_explain_analyze_sql(&self, database_type: DatabaseType, query: &str)
    -> Option<String>;
    fn build_update_sql(
        &self,
        database_type: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String;
    fn build_bulk_delete_sql(
        &self,
        database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String;
}
