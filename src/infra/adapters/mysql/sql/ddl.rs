use crate::domain::Table;

use super::super::MySqlAdapter;

impl MySqlAdapter {
    pub fn generate_ddl(&self, table: &Table) -> String {
        table.source_ddl().unwrap_or_default().to_string()
    }
}
