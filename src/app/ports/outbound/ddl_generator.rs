use crate::domain::{DatabaseType, Table};

pub trait DdlGenerator: Send + Sync {
    fn generate_ddl(&self, database_type: DatabaseType, table: &Table) -> String;
}
