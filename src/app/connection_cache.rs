//! Per-connection state cache for seamless switching.
//!
//! Preserves user context (selected table, query results, etc.) across connection switches.

use std::collections::HashMap;
use std::sync::Arc;

use crate::app::inspector_tab::InspectorTab;
use crate::app::result_history::ResultHistory;
use crate::domain::{ConnectionId, DatabaseMetadata, QueryResult, Table};

#[derive(Debug, Clone, Default)]
pub struct ConnectionCache {
    pub metadata: Option<DatabaseMetadata>,
    pub table_detail: Option<Table>,
    pub current_table: Option<String>,
    pub query_result: Option<Arc<QueryResult>>,
    pub result_history: ResultHistory,
    pub explorer_selected: usize,
    pub inspector_tab: InspectorTab,
}

#[derive(Debug, Default)]
pub struct ConnectionCacheStore {
    caches: HashMap<ConnectionId, ConnectionCache>,
}

impl ConnectionCacheStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&mut self, id: &ConnectionId) -> &mut ConnectionCache {
        self.caches.entry(id.clone()).or_default()
    }

    pub fn get(&self, id: &ConnectionId) -> Option<&ConnectionCache> {
        self.caches.get(id)
    }

    pub fn save(&mut self, id: &ConnectionId, cache: ConnectionCache) {
        self.caches.insert(id.clone(), cache);
    }

    pub fn remove(&mut self, id: &ConnectionId) -> Option<ConnectionCache> {
        self.caches.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_cache_default_has_empty_fields() {
        let cache = ConnectionCache::default();

        assert!(cache.metadata.is_none());
        assert!(cache.table_detail.is_none());
        assert!(cache.current_table.is_none());
        assert!(cache.query_result.is_none());
        assert_eq!(cache.explorer_selected, 0);
        assert_eq!(cache.inspector_tab, InspectorTab::default());
    }

    #[test]
    fn store_get_returns_none_for_unknown_id() {
        let store = ConnectionCacheStore::new();
        let id = ConnectionId::new();

        assert!(store.get(&id).is_none());
    }

    #[test]
    fn store_get_or_create_creates_default() {
        let mut store = ConnectionCacheStore::new();
        let id = ConnectionId::new();

        let cache = store.get_or_create(&id);
        assert!(cache.metadata.is_none());
    }

    #[test]
    fn store_save_and_get_returns_saved_cache() {
        let mut store = ConnectionCacheStore::new();
        let id = ConnectionId::new();

        let cache = ConnectionCache {
            explorer_selected: 42,
            inspector_tab: InspectorTab::Indexes,
            ..Default::default()
        };
        store.save(&id, cache);

        let retrieved = store.get(&id).unwrap();
        assert_eq!(retrieved.explorer_selected, 42);
        assert_eq!(retrieved.inspector_tab, InspectorTab::Indexes);
    }

    #[test]
    fn store_remove_returns_and_deletes_cache() {
        let mut store = ConnectionCacheStore::new();
        let id = ConnectionId::new();

        let cache = ConnectionCache {
            explorer_selected: 99,
            ..Default::default()
        };
        store.save(&id, cache);

        let removed = store.remove(&id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().explorer_selected, 99);
        assert!(store.get(&id).is_none());
    }
}
