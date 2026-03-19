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

#[cfg(test)]
mod tests {
    use super::*;

    mod tables_by_schema {
        use super::*;

        #[test]
        fn multiple_schemas_groups_correctly() {
            let mut meta = DatabaseMetadata::new("testdb".to_string());
            meta.tables = vec![
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
                TableSummary::new("public".to_string(), "orders".to_string(), None, false),
                TableSummary::new("audit".to_string(), "logs".to_string(), None, false),
            ];

            let grouped = meta.tables_by_schema();

            assert_eq!(grouped.len(), 2);
            assert_eq!(grouped["public"].len(), 2);
            assert_eq!(grouped["audit"].len(), 1);
        }

        #[test]
        fn empty_tables_returns_empty_map() {
            let meta = DatabaseMetadata::new("testdb".to_string());

            assert!(meta.tables_by_schema().is_empty());
        }
    }
}
