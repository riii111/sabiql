use crate::app::ports::outbound::DdlGenerator;
use crate::domain::{DatabaseType, Table};

use super::super::MySqlAdapter;

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        table.source_ddl().unwrap_or_default().to_string()
    }
}
