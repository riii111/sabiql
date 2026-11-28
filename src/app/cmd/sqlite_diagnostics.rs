use tokio::task::JoinHandle;

use std::sync::Arc;

use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
use crate::ports::outbound::SqliteDiagnosticsProvider;
use crate::update::action::Action;

#[derive(Default)]
pub(in crate::cmd) struct SqliteDiagnosticsTaskOwner {
    pub(in crate::cmd) core: SingleTaskOwner,
    pub(in crate::cmd) quick_check: SingleTaskOwner,
}

impl SqliteDiagnosticsTaskOwner {
    pub(in crate::cmd) async fn cancel(&self) {
        for task in self.abort() {
            let _ = task.await;
        }
    }

    pub(in crate::cmd) fn abort(&self) -> Vec<JoinHandle<()>> {
        let core = self.core.abort();
        let quick_check = self.quick_check.abort();
        [core, quick_check].into_iter().flatten().collect()
    }
}

pub(in crate::cmd) async fn fetch_core(
    provider: Arc<dyn SqliteDiagnosticsProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> Action {
    let snapshot = provider
        .fetch_core_diagnostics(&endpoint)
        .await
        .unwrap_or_else(|error| {
            SqliteDiagnosticsSnapshot::core_fetch_failed(DiagnosticField::err(
                error.masked_details(),
            ))
        });
    Action::SqliteDiagnosticsCoreLoaded {
        run_id,
        snapshot: Box::new(snapshot),
    }
}

pub(in crate::cmd) async fn fetch_quick_check(
    provider: Arc<dyn SqliteDiagnosticsProvider>,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
) -> Action {
    let quick_check = provider.fetch_quick_check(&endpoint).await;
    Action::SqliteDiagnosticsQuickCheckLoaded {
        run_id,
        quick_check,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::outbound::DbOperationError;
    use crate::ports::outbound::sqlite_diagnostics::MockSqliteDiagnosticsProvider;
    use crate::test_support::endpoint;

    fn app_db() -> Arc<ConnectionEndpoint> {
        endpoint::sqlite("sqlite:///tmp/app.db")
    }

    #[tokio::test]
    async fn core_reports_snapshot_on_success() {
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider.expect_fetch_core_diagnostics().returning(|_| {
            Ok(SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            })
        });

        let action = fetch_core(Arc::new(provider), app_db(), 1).await;

        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.sqlite_version.ok_value() == Some("3.45.0")
        ));
    }

    #[tokio::test]
    async fn quick_check_reports_field() {
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_quick_check()
            .returning(|_| DiagnosticField::ok("ok"));

        let action = fetch_quick_check(Arc::new(provider), app_db(), 1).await;

        assert!(matches!(
            action,
            Action::SqliteDiagnosticsQuickCheckLoaded {
                quick_check,
                ..
            } if quick_check.ok_value() == Some("ok")
        ));
    }

    #[tokio::test]
    async fn core_reports_partial_snapshot_on_provider_error() {
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_core_diagnostics()
            .returning(|_| Err(DbOperationError::QueryFailed("boom".to_string())));

        let action = fetch_core(Arc::new(provider), app_db(), 1).await;

        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.db_file.is_err()
        ));
    }
}
