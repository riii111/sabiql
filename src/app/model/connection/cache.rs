use std::sync::Arc;

use crate::domain::{DatabaseMetadata, QueryResult, Table};
use crate::model::browse::query_execution::PaginationState;
use crate::model::shared::inspector_tab::InspectorTab;

#[derive(Debug, Clone, Default)]
pub struct ConnectionCache {
    pub connection_dsn: Option<String>,
    pub metadata: Option<Arc<DatabaseMetadata>>,
    pub effective_user: Option<String>,
    pub table_detail: Option<Table>,
    pub selected_table_key: Option<String>,
    pub query_result: Option<Arc<QueryResult>>,
    pub pagination: PaginationState,
    pub explorer_selected: usize,
    pub inspector_tab: InspectorTab,
}

impl ConnectionCache {
    pub fn is_valid_mysql_snapshot(&self, dsn: &str) -> bool {
        self.connection_dsn.as_deref() == Some(dsn)
            && self.metadata.is_some()
            && self.effective_user.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_cache_default_has_empty_fields() {
        let cache = ConnectionCache::default();

        assert!(cache.metadata.is_none());
        assert!(cache.effective_user.is_none());
        assert!(cache.connection_dsn.is_none());
        assert!(cache.table_detail.is_none());
        assert!(cache.selected_table_key.is_none());
        assert!(cache.query_result.is_none());
        assert_eq!(cache.pagination.current_page(), 0);
        assert!(cache.pagination.schema().is_empty());
        assert!(cache.pagination.table().is_empty());
        assert_eq!(cache.explorer_selected, 0);
        assert_eq!(cache.inspector_tab, InspectorTab::default());
    }

    #[test]
    fn mysql_restore_requires_matching_dsn_metadata_and_effective_user() {
        let cache = ConnectionCache {
            connection_dsn: Some("mysql://user@localhost/app".to_string()),
            metadata: Some(Arc::new(DatabaseMetadata::new("app".to_string()))),
            effective_user: Some("user@localhost".to_string()),
            ..Default::default()
        };

        assert!(cache.is_valid_mysql_snapshot("mysql://user@localhost/app"));
        assert!(!cache.is_valid_mysql_snapshot("mysql://user@localhost/other"));

        let mut cache_without_metadata = cache.clone();
        cache_without_metadata.metadata = None;
        assert!(!cache_without_metadata.is_valid_mysql_snapshot("mysql://user@localhost/app"));

        let mut cache_without_effective_user = cache;
        cache_without_effective_user.effective_user = None;
        assert!(
            !cache_without_effective_user.is_valid_mysql_snapshot("mysql://user@localhost/app")
        );
    }
}
