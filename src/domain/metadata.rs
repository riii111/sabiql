use super::schema::Schema;
use super::table::TableSummary;

#[derive(Debug, Clone)]
pub struct DatabaseMetadata {
    pub database_name: String,
    pub schemas: Vec<Schema>,
    pub table_summaries: Vec<TableSummary>,
}

impl DatabaseMetadata {
    #[must_use]
    pub fn new(database_name: String) -> Self {
        Self {
            database_name,
            schemas: Vec::new(),
            table_summaries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MetadataState {
    #[default]
    NotLoaded,
    Loading,
    Loaded,
    Error(String),
}
