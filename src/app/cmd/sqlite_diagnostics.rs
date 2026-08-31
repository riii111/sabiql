use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
use crate::ports::outbound::SqliteDiagnosticsProvider;
use crate::update::action::Action;

#[derive(Default)]
pub(crate) struct SqliteDiagnosticsTaskOwner {
    core: SingleTaskOwner,
    quick_check: SingleTaskOwner,
}

impl SqliteDiagnosticsTaskOwner {
    pub(crate) async fn cancel(&self) {
        for task in self.abort() {
            let _ = task.await;
        }
    }

    pub(crate) fn abort(&self) -> Vec<JoinHandle<()>> {
        let core = self.core.abort();
        let quick_check = self.quick_check.abort();
        [core, quick_check].into_iter().flatten().collect()
    }
}

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    provider: &Arc<dyn SqliteDiagnosticsProvider>,
    task_owner: &SqliteDiagnosticsTaskOwner,
) {
    match effect {
        Effect::FetchSqliteDiagnosticsCore { dsn, run_id } => {
            let action_tx = action_tx.clone();
            let provider = Arc::clone(provider);
            task_owner
                .core
                .replace(async move {
                    let action = match provider.fetch_core_diagnostics(&dsn).await {
                        Ok(snapshot) => Action::SqliteDiagnosticsCoreLoaded {
                            dsn,
                            run_id,
                            snapshot: Box::new(snapshot),
                        },
                        Err(error) => Action::SqliteDiagnosticsCoreLoaded {
                            dsn,
                            run_id,
                            snapshot: Box::new(SqliteDiagnosticsSnapshot::core_fetch_failed(
                                DiagnosticField::err(error.masked_details()),
                            )),
                        },
                    };
                    let _ = action_tx.send(action).await;
                })
                .await;
        }
        Effect::FetchSqliteDiagnosticsQuickCheck { dsn, run_id } => {
            let action_tx = action_tx.clone();
            let provider = Arc::clone(provider);
            task_owner
                .quick_check
                .replace(async move {
                    let quick_check = provider.fetch_quick_check(&dsn).await;
                    let _ = action_tx
                        .send(Action::SqliteDiagnosticsQuickCheckLoaded {
                            dsn,
                            run_id,
                            quick_check,
                        })
                        .await;
                })
                .await;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::outbound::DbOperationError;
    use crate::ports::outbound::sqlite_diagnostics::MockSqliteDiagnosticsProvider;

    #[tokio::test]
    async fn dispatches_core_snapshot_on_success() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider.expect_fetch_core_diagnostics().returning(|_| {
            Ok(SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            })
        });

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        let owner = SqliteDiagnosticsTaskOwner::default();
        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.sqlite_version.ok_value() == Some("3.45.0")
        ));
    }

    #[tokio::test]
    async fn dispatches_quick_check_field_on_success() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_quick_check()
            .returning(|_| DiagnosticField::ok("ok"));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        let owner = SqliteDiagnosticsTaskOwner::default();
        run(
            Effect::FetchSqliteDiagnosticsQuickCheck {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsQuickCheckLoaded {
                quick_check,
                ..
            } if quick_check.ok_value() == Some("ok")
        ));
    }

    #[tokio::test]
    async fn core_and_quick_check_tasks_can_run_together() {
        use tokio::time::{Duration, timeout};

        let (tx, mut rx) = mpsc::channel(2);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_core_diagnostics()
            .returning(|_| Ok(SqliteDiagnosticsSnapshot::default()));
        provider
            .expect_fetch_quick_check()
            .returning(|_| DiagnosticField::ok("ok"));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        let owner = SqliteDiagnosticsTaskOwner::default();
        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;
        run(
            Effect::FetchSqliteDiagnosticsQuickCheck {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;

        let first = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("core and quick-check tasks should both complete")
            .expect("first diagnostics action should be sent");
        let second = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("core and quick-check tasks should both complete")
            .expect("second diagnostics action should be sent");
        assert!(matches!(
            (first, second),
            (
                Action::SqliteDiagnosticsCoreLoaded { .. },
                Action::SqliteDiagnosticsQuickCheckLoaded { .. }
            ) | (
                Action::SqliteDiagnosticsQuickCheckLoaded { .. },
                Action::SqliteDiagnosticsCoreLoaded { .. }
            )
        ));
    }

    #[tokio::test]
    async fn dispatches_partial_core_snapshot_on_provider_error() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_core_diagnostics()
            .returning(|_| Err(DbOperationError::QueryFailed("boom".to_string())));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        let owner = SqliteDiagnosticsTaskOwner::default();
        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.db_file.is_err()
        ));
    }

    struct BlockingProvider {
        started: Arc<tokio::sync::Notify>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Drop for BlockingProvider {
        fn drop(&mut self) {
            self.dropped
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl SqliteDiagnosticsProvider for BlockingProvider {
        async fn fetch_core_diagnostics(
            &self,
            _dsn: &str,
        ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
            self.started.notify_one();
            std::future::pending().await
        }

        async fn fetch_quick_check(&self, _dsn: &str) -> DiagnosticField {
            self.started.notify_one();
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn cancel_drops_provider_and_sends_no_late_action() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::time::{Duration, timeout};

        let (tx, mut rx) = mpsc::channel(1);
        let started = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let provider = Arc::new(BlockingProvider {
            started: Arc::clone(&started),
            dropped: Arc::clone(&dropped),
        }) as Arc<dyn SqliteDiagnosticsProvider>;
        let owner = SqliteDiagnosticsTaskOwner::default();

        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
            },
            &tx,
            &provider,
            &owner,
        )
        .await;
        timeout(Duration::from_secs(1), started.notified())
            .await
            .expect("diagnostics provider should start");

        owner.cancel().await;
        drop(provider);

        assert!(dropped.load(Ordering::SeqCst));
        assert!(rx.try_recv().is_err());
    }
}
