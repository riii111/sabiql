use std::sync::Arc;

use crate::domain::query_history::QueryHistoryScope;
use crate::ports::outbound::QueryHistoryStore;
use crate::update::action::Action;

pub(in crate::cmd) async fn load_query_history(
    store: Arc<dyn QueryHistoryStore>,
    project_name: String,
    scope: QueryHistoryScope,
) -> Action {
    let outcome = store
        .load(&project_name, &scope)
        .await
        .map_err(|error| error.to_string());
    Action::QueryHistoryLoaded(scope, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::ConnectionId;
    use crate::domain::query_history::{QueryHistoryEntry, QueryResultStatus};
    use crate::ports::outbound::QueryHistoryError;

    struct TestQueryHistoryStore {
        result: Result<Vec<QueryHistoryEntry>, QueryHistoryError>,
    }

    #[async_trait::async_trait]
    impl QueryHistoryStore for TestQueryHistoryStore {
        async fn append(
            &self,
            _project_name: &str,
            _scope: &QueryHistoryScope,
            _entry: &QueryHistoryEntry,
        ) -> Result<(), QueryHistoryError> {
            Ok(())
        }

        async fn load(
            &self,
            _project_name: &str,
            _scope: &QueryHistoryScope,
        ) -> Result<Vec<QueryHistoryEntry>, QueryHistoryError> {
            self.result.clone()
        }
    }

    fn scope() -> QueryHistoryScope {
        QueryHistoryScope::new(ConnectionId::from_string("test-conn"), None)
    }

    fn entry() -> QueryHistoryEntry {
        QueryHistoryEntry::new_with_database(
            "SELECT 1".to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            None,
            QueryResultStatus::Success,
            None,
        )
    }

    #[tokio::test]
    async fn load_reports_store_outcome_for_scope() {
        let entries = vec![entry()];
        let loaded = load_query_history(
            Arc::new(TestQueryHistoryStore {
                result: Ok(entries.clone()),
            }),
            "test".to_string(),
            scope(),
        )
        .await;
        let failed = load_query_history(
            Arc::new(TestQueryHistoryStore {
                result: Err(QueryHistoryError::Io(Arc::new(std::io::Error::other(
                    "disk error",
                )))),
            }),
            "test".to_string(),
            scope(),
        )
        .await;

        match loaded {
            Action::QueryHistoryLoaded(received_scope, Ok(received_entries)) => {
                assert_eq!(received_scope, scope());
                assert_eq!(received_entries, entries);
            }
            other => panic!("expected loaded history, got {other:?}"),
        }
        match failed {
            Action::QueryHistoryLoaded(received_scope, Err(error)) => {
                assert_eq!(received_scope, scope());
                assert_eq!(error, "IO error: disk error");
            }
            other => panic!("expected failed history, got {other:?}"),
        }
    }
}
