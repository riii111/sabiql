use crate::domain::{DatabaseMetadata, MetadataState, Table, TableSummary};

#[derive(Debug, Clone, Default)]
pub struct MetadataCache {
    pub state: MetadataState,
    pub metadata: Option<DatabaseMetadata>,
    pub table_detail: Option<Table>,
    pub current_table: Option<String>,
    pub selection_generation: u64,
}

impl MetadataCache {
    pub fn tables(&self) -> Vec<&TableSummary> {
        self.metadata
            .as_ref()
            .map(|m| m.tables.iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_creates_empty_cache() {
        let cache = MetadataCache::default();

        assert_eq!(cache.state, MetadataState::NotLoaded);
        assert!(cache.metadata.is_none());
        assert!(cache.table_detail.is_none());
        assert!(cache.current_table.is_none());
        assert_eq!(cache.selection_generation, 0);
    }

    #[test]
    fn tables_returns_empty_when_no_metadata() {
        let cache = MetadataCache::default();

        let tables = cache.tables();

        assert!(tables.is_empty());
    }

    #[test]
    fn tables_returns_all_when_metadata_exists() {
        let mut cache = MetadataCache::default();
        cache.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: std::time::Instant::now(),
        });

        let tables = cache.tables();

        assert_eq!(tables.len(), 2);
    }
}
