use crate::domain::Table;

pub trait DdlGenerator: Send + Sync {
    fn generate_ddl(&self, table: &Table) -> String;
    fn ddl_line_count(&self, table: &Table) -> usize;
}
