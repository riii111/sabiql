use crate::app::ports::outbound::SqlDialect;
use crate::domain::{DatabaseType, QueryValue};

use super::super::MySqlAdapter;
use super::{explain, grid_write};

impl SqlDialect for MySqlAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        explain::build_explain_sql(query)
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        explain::build_explain_analyze_sql(query)
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
        grid_write::build_update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        grid_write::build_bulk_delete_sql(schema, table, pk_pairs_per_row)
    }
}
