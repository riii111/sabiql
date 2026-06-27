use tokio::sync::mpsc;

use std::sync::Arc;

use crate::cmd::effect::Effect;
use crate::ports::outbound::SqliteDiagnosticsProvider;
use crate::update::action::Action;

pub fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    provider: &Arc<dyn SqliteDiagnosticsProvider>,
) {
    let Effect::FetchSqliteDiagnostics {
        dsn,
        run_id,
        read_only,
    } = effect
    else {
        return;
    };

    let action_tx = action_tx.clone();
    let provider = Arc::clone(provider);
    tokio::spawn(async move {
        let action = match provider.fetch_diagnostics(&dsn, read_only).await {
            Ok(snapshot) => Action::SqliteDiagnosticsLoaded {
                dsn,
                run_id,
                snapshot: Box::new(snapshot),
            },
            Err(error) => Action::SqliteDiagnosticsLoaded {
                dsn,
                run_id,
                snapshot: Box::new(crate::domain::SqliteDiagnosticsSnapshot {
                    db_file: crate::domain::DiagnosticField::err(error.masked_details()),
                    ..Default::default()
                }),
            },
        };
        let _ = action_tx.send(action).await;
    });
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
    async fn dispatches_loaded_snapshot_on_success() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider.expect_fetch_diagnostics().returning(|_, _| {
            Ok(SqliteDiagnosticsSnapshot {
                sqlite_version: DiagnosticField::ok("3.45.0"),
                ..Default::default()
            })
        });

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        run(
            Effect::FetchSqliteDiagnostics {
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
            Action::SqliteDiagnosticsLoaded {
                snapshot,
                ..
            } if snapshot.sqlite_version.value.as_deref() == Some("3.45.0")
        ));
    }

    #[tokio::test]
    async fn dispatches_partial_snapshot_on_provider_error() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut provider = MockSqliteDiagnosticsProvider::new();
        provider
            .expect_fetch_diagnostics()
            .returning(|_, _| Err(DbOperationError::QueryFailed("boom".to_string())));

        let provider = Arc::new(provider) as Arc<dyn SqliteDiagnosticsProvider>;
        run(
            Effect::FetchSqliteDiagnostics {
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
            Action::SqliteDiagnosticsLoaded {
                snapshot,
                ..
            } if snapshot.db_file.error.is_some()
        ));
    }
}
