use crate::domain::{DatabaseType, Table};

pub trait DdlGenerator: Send + Sync {
    fn generate_ddl(&self, database_type: DatabaseType, table: &Table) -> String;
    fn ddl_line_count(&self, database_type: DatabaseType, table: &Table) -> usize {
        self.generate_ddl(database_type, table).lines().count()
    }
}
