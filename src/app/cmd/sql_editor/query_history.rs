use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::ports::outbound::QueryHistoryStore;
use crate::update::action::Action;

pub fn spawn_query_history_load(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    query_history_store: &Arc<dyn QueryHistoryStore>,
) {
    match effect {
        Effect::LoadQueryHistory {
            project_name,
            scope,
        } => {
            let store = Arc::clone(query_history_store);
            let tx = action_tx.clone();

            tokio::spawn(async move {
                let action = match store.load(&project_name, &scope).await {
                    Ok(entries) => Action::QueryHistoryLoaded(scope, entries),
                    Err(e) => Action::QueryHistoryLoadFailed(scope, e.to_string()),
                };
                tx.send(action).await.ok();
            });
        }
        _ => unreachable!("spawn_query_history_load called with non-query-history effect"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::ConnectionId;
    use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope, QueryResultStatus};
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
    async fn load_success_dispatches_one_loaded_action() {
        let scope = scope();
        let entries = vec![entry()];
        let store: Arc<dyn QueryHistoryStore> = Arc::new(TestQueryHistoryStore {
            result: Ok(entries.clone()),
        });
        let (tx, mut rx) = mpsc::channel(2);

        spawn_query_history_load(
            Effect::LoadQueryHistory {
                project_name: "test".to_string(),
                scope: scope.clone(),
            },
            &tx,
            &store,
        );

        let action = rx.recv().await.expect("action dispatched");
        match action {
            Action::QueryHistoryLoaded(received_scope, received_entries) => {
                assert_eq!(received_scope, scope);
                assert_eq!(received_entries, entries);
            }
            other => panic!("expected QueryHistoryLoaded, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn load_failure_dispatches_one_display_error_action() {
        let scope = scope();
        let store: Arc<dyn QueryHistoryStore> = Arc::new(TestQueryHistoryStore {
            result: Err(QueryHistoryError::Io(Arc::new(std::io::Error::other(
                "disk error",
            )))),
        });
        let (tx, mut rx) = mpsc::channel(2);

        spawn_query_history_load(
            Effect::LoadQueryHistory {
                project_name: "test".to_string(),
                scope: scope.clone(),
            },
            &tx,
            &store,
        );

        let action = rx.recv().await.expect("action dispatched");
        match action {
            Action::QueryHistoryLoadFailed(received_scope, error) => {
                assert_eq!(received_scope, scope);
                assert_eq!(error, "IO error: disk error");
            }
            other => panic!("expected QueryHistoryLoadFailed, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());
    }
}
