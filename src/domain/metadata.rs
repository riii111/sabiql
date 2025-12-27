use super::schema::Schema;
use super::table::TableSummary;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct DatabaseMetadata {
    pub database_name: String,
    pub schemas: Vec<Schema>,
    pub tables: Vec<TableSummary>,
    pub fetched_at: Instant,
}

impl DatabaseMetadata {
    pub fn new(database_name: String) -> Self {
        Self {
            database_name,
            schemas: Vec::new(),
            tables: Vec::new(),
            fetched_at: Instant::now(),
        }
    }

    pub fn tables_by_schema(&self) -> HashMap<&str, Vec<&TableSummary>> {
        let mut map: HashMap<&str, Vec<&TableSummary>> = HashMap::new();
        for table in &self.tables {
            map.entry(&table.schema).or_default().push(table);
        }
        map
    }

    pub fn age_seconds(&self) -> u64 {
        self.fetched_at.elapsed().as_secs()
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
