use tokio::sync::mpsc;

use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::domain::DiagnosticField;
use crate::ports::outbound::SqliteDiagnosticsProvider;
use crate::update::action::Action;

pub fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    provider: &Arc<dyn SqliteDiagnosticsProvider>,
) {
    match effect {
        Effect::FetchSqliteDiagnosticsCore {
            dsn,
            run_id,
            read_only,
        } => {
            let action_tx = action_tx.clone();
            let provider = Arc::clone(provider);
            tokio::spawn(async move {
                let action = match provider.fetch_diagnostics_core(&dsn, read_only).await {
                    Ok(snapshot) => Action::SqliteDiagnosticsCoreLoaded {
                        dsn,
                        run_id,
                        snapshot: Box::new(snapshot),
                    },
                    Err(error) => Action::SqliteDiagnosticsCoreLoaded {
                        dsn,
                        run_id,
                        snapshot: Box::new(crate::domain::SqliteDiagnosticsSnapshot {
                            db_file: DiagnosticField::err(error.masked_details()),
                            ..Default::default()
                        }),
                    },
                };
                let _ = action_tx.send(action).await;
            });
        }
        Effect::FetchSqliteDiagnosticsQuickCheck {
            dsn,
            run_id,
            read_only,
        } => {
            let action_tx = action_tx.clone();
            let provider = Arc::clone(provider);
            tokio::spawn(async move {
                let quick_check = match provider.fetch_quick_check(&dsn, read_only).await {
                    Ok(field) => field,
                    Err(error) => DiagnosticField::err(error.masked_details()),
                };
                let _ = action_tx
                    .send(Action::SqliteDiagnosticsQuickCheckLoaded {
                        dsn,
                        run_id,
                        quick_check,
                    })
                    .await;
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
    use crate::ports::outbound::DbOperationError;
    use crate::ports::outbound::sqlite_diagnostics::MockSqliteDiagnosticsProvider;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn dispatches_core_snapshot_on_success() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider.expect_fetch_diagnostics_core().returning(|_, _| {
            Ok(SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            })
        });

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
                read_only: true,
            },
            &tx,
            &provider,
        );

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.sqlite_version.value.as_deref() == Some("3.45.0")
        ));
    }

    #[tokio::test]
    async fn dispatches_quick_check_field_on_success() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_quick_check()
            .returning(|_, _| Ok(DiagnosticField::ok("ok")));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        run(
            Effect::FetchSqliteDiagnosticsQuickCheck {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
                read_only: true,
            },
            &tx,
            &provider,
        );

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsQuickCheckLoaded {
                quick_check,
                ..
            } if quick_check.value.as_deref() == Some("ok")
        ));
    }

    #[tokio::test]
    async fn dispatches_partial_core_snapshot_on_provider_error() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_diagnostics_core()
            .returning(|_, _| Err(DbOperationError::QueryFailed("boom".to_string())));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        run(
            Effect::FetchSqliteDiagnosticsCore {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: 1,
                read_only: true,
            },
            &tx,
            &provider,
        );

        let action = rx.recv().await.unwrap();
        assert!(matches!(
            action,
            Action::SqliteDiagnosticsCoreLoaded {
                snapshot,
                ..
            } if snapshot.db_file.error.is_some()
        ));
    }
}
