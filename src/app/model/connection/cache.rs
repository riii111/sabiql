use std::sync::Arc;

use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{DatabaseMetadata, QueryResult, Table};
use crate::model::browse::query_execution::PaginationState;
use crate::model::shared::inspector_tab::InspectorTab;

#[derive(Debug, Clone, Default)]
pub struct ConnectionCache {
    pub endpoint: Option<Arc<ConnectionEndpoint>>,
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
    pub fn is_valid_mysql_snapshot(&self, endpoint: &ConnectionEndpoint) -> bool {
        self.endpoint.as_deref() == Some(endpoint)
            && self.metadata.is_some()
            && self.effective_user.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::endpoint;

    #[test]
    fn mysql_restore_requires_matching_dsn_metadata_and_effective_user() {
        let cache = ConnectionCache {
            endpoint: Some(endpoint::mysql("mysql://user@localhost/app", Some("app"))),
            metadata: Some(Arc::new(DatabaseMetadata::new("app".to_string()))),
            effective_user: Some("user@localhost".to_string()),
            ..Default::default()
        };

        assert!(
            cache.is_valid_mysql_snapshot(&endpoint::mysql(
                "mysql://user@localhost/app",
                Some("app")
            ))
        );
        assert!(!cache.is_valid_mysql_snapshot(&endpoint::mysql(
            "mysql://user@localhost/other",
            Some("other")
        )));

        let mut cache_without_metadata = cache.clone();
        cache_without_metadata.metadata = None;
        assert!(
            !cache_without_metadata.is_valid_mysql_snapshot(&endpoint::mysql(
                "mysql://user@localhost/app",
                Some("app")
            ))
        );

        let mut cache_without_effective_user = cache;
        cache_without_effective_user.effective_user = None;
        assert!(
            !cache_without_effective_user.is_valid_mysql_snapshot(&endpoint::mysql(
                "mysql://user@localhost/app",
                Some("app")
            ))
        );
    }
}
