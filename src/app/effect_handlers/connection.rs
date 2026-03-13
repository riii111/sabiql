use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::action::{Action, ConnectionTarget, ConnectionsLoadedPayload};
use crate::app::cache::TtlCache;
use crate::app::effect::Effect;
use crate::app::ports::{
    ConnectionStore, DsnBuilder, MetadataProvider, ServiceFileError, ServiceFileReader,
};
use crate::app::state::AppState;
use crate::domain::DatabaseMetadata;
use crate::domain::connection::ConnectionProfile;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    dsn_builder: &Arc<dyn DsnBuilder>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, Arc<DatabaseMetadata>>,
    connection_store: &Arc<dyn ConnectionStore>,
    service_file_reader: &Arc<dyn ServiceFileReader>,
    state: &mut AppState,
) -> Result<()> {
    match effect {
        Effect::SaveAndConnect {
            id,
            name,
            host,
            port,
            database,
            user,
            password,
            ssl_mode,
        } => {
            let profile = match id {
                Some(existing_id) => {
                    match ConnectionProfile::with_id(
                        existing_id,
                        name,
                        host,
                        port,
                        database,
                        user,
                        password,
                        ssl_mode,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            action_tx
                                .blocking_send(Action::ConnectionSaveFailed(e.to_string()))
                                .ok();
                            return Ok(());
                        }
                    }
                }
                None => {
                    match ConnectionProfile::new(
                        name, host, port, database, user, password, ssl_mode,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            action_tx
                                .blocking_send(Action::ConnectionSaveFailed(e.to_string()))
                                .ok();
                            return Ok(());
                        }
                    }
                }
            };
            let id = profile.id.clone();
            let dsn = dsn_builder.build_dsn(&profile);
            let name = profile.name.as_str().to_string();
            let store = Arc::clone(connection_store);
            let tx = action_tx.clone();

            let provider = Arc::clone(metadata_provider);
            let cache = metadata_cache.clone();
            tokio::spawn(async move {
                match provider.fetch_metadata(&dsn).await {
                    Ok(metadata) => {
                        cache.set(dsn.clone(), Arc::new(metadata)).await;
                        match store.save(&profile) {
                            Ok(()) => {
                                tx.send(Action::ConnectionSaveCompleted(ConnectionTarget {
                                    id,
                                    dsn,
                                    name,
                                }))
                                .await
                                .ok();
                            }
                            Err(e) => {
                                cache.invalidate(&dsn).await;
                                tx.send(Action::ConnectionSaveFailed(e.to_string()))
                                    .await
                                    .ok();
                            }
                        }
                    }
                    Err(e) => {
                        tx.send(Action::MetadataFailed(e.to_string())).await.ok();
                    }
                }
            });
            Ok(())
        }

        Effect::LoadConnectionForEdit { id } => {
            let store = Arc::clone(connection_store);
            let tx = action_tx.clone();

            tokio::task::spawn_blocking(move || match store.find_by_id(&id) {
                Ok(Some(profile)) => {
                    tx.blocking_send(Action::ConnectionEditLoaded(Box::new(profile)))
                        .ok();
                }
                Ok(None) => {
                    tx.blocking_send(Action::ConnectionEditLoadFailed(
                        "Connection not found".to_string(),
                    ))
                    .ok();
                }
                Err(e) => {
                    tx.blocking_send(Action::ConnectionEditLoadFailed(e.to_string()))
                        .ok();
                }
            });
            Ok(())
        }

        Effect::LoadConnections => {
            let store = Arc::clone(connection_store);
            let reader = Arc::clone(service_file_reader);
            let tx = action_tx.clone();

            tokio::task::spawn_blocking(move || {
                let (profiles, profile_load_warning) = match store.load_all() {
                    Ok(p) => (p, None),
                    Err(e) => (vec![], Some(e.to_string())),
                };
                let (services, service_file_path, service_load_warning) =
                    match reader.read_services() {
                        Ok((s, p)) => (s, Some(p), None),
                        Err(ServiceFileError::NotFound(_)) => (vec![], None, None),
                        Err(e) => (vec![], None, Some(e.to_string())),
                    };

                tx.blocking_send(Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services,
                    service_file_path,
                    profile_load_warning,
                    service_load_warning,
                }))
                .ok();
            });
            Ok(())
        }

        Effect::DeleteConnection { id } => {
            let store = Arc::clone(connection_store);
            let tx = action_tx.clone();

            tokio::task::spawn_blocking(move || match store.delete(&id) {
                Ok(()) => {
                    tx.blocking_send(Action::ConnectionDeleted(id)).ok();
                }
                Err(e) => {
                    tx.blocking_send(Action::ConnectionDeleteFailed(e.to_string()))
                        .ok();
                }
            });
            Ok(())
        }

        Effect::SwitchConnection { connection_index } => {
            if let Some(profile) = state.connections().get(connection_index) {
                let dsn = dsn_builder.build_dsn(profile);
                let name = profile.display_name().to_string();
                let id = profile.id.clone();
                action_tx
                    .send(Action::SwitchConnection(ConnectionTarget { id, dsn, name }))
                    .await
                    .ok();
            }
            Ok(())
        }

        Effect::SwitchToService { service_index } => {
            if let Some(entry) = state.service_entries().get(service_index) {
                let id = entry.connection_id();
                let dsn = entry.to_dsn();
                let name = entry.display_name().to_owned();
                action_tx
                    .send(Action::SwitchConnection(ConnectionTarget { id, dsn, name }))
                    .await
                    .ok();
            }
            Ok(())
        }

        _ => unreachable!("connection::run called with non-connection effect"),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use super::super::test_support::*;
    use crate::app::action::{Action, ConnectionTarget, ConnectionsLoadedPayload};
    use crate::app::cache::TtlCache;
    use crate::app::completion::CompletionEngine;
    use crate::app::effect::Effect;
    use crate::app::ports::connection_store::MockConnectionStore;
    use crate::app::ports::metadata::MockMetadataProvider;
    use crate::app::ports::query_executor::MockQueryExecutor;
    use crate::app::ports::{DsnBuilder, RenderOutput, Renderer};
    use crate::app::services::AppServices;
    use crate::app::state::AppState;
    use crate::domain::connection::{ConnectionId, ConnectionProfile, SslMode};
    use color_eyre::eyre::Result;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(&mut self, _state: &mut AppState, _services: &AppServices) -> Result<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    mod delete_connection {
        use super::*;

        #[tokio::test]
        async fn success_returns_connection_deleted() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_delete().once().returning(|_| Ok(()));

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                cache,
                tx,
            );

            let id = ConnectionId::new();
            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::DeleteConnection { id: id.clone() }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::ConnectionDeleted(_)),
                "expected ConnectionDeleted, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn error_returns_connection_delete_failed() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_delete().once().returning(|_| {
                Err(crate::app::ports::ConnectionStoreError::NotFound(
                    "id".to_string(),
                ))
            });

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                cache,
                tx,
            );

            let id = ConnectionId::new();
            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::DeleteConnection { id }],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::ConnectionDeleteFailed(_)),
                "expected ConnectionDeleteFailed, got {:?}",
                action
            );
        }
    }

    mod load_connections {
        use super::*;

        #[tokio::test]
        async fn error_returns_empty_connections_list() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_load_all().once().returning(|| {
                Err(crate::app::ports::ConnectionStoreError::ReadError(
                    "file not found".to_string(),
                ))
            });

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::LoadConnections],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::ConnectionsLoaded(ConnectionsLoadedPayload { ref profiles, .. }) if profiles.is_empty()),
                "expected ConnectionsLoaded with empty profiles, got {:?}",
                action
            );
        }
    }

    mod switch_connection {
        use super::*;

        struct FakeDsnBuilder;
        impl DsnBuilder for FakeDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                format!(
                    "fake://{}:{}/{}",
                    profile.host, profile.port, profile.database
                )
            }
        }

        fn make_runner_with_dsn_builder(
            action_tx: mpsc::Sender<Action>,
        ) -> crate::app::effect_runner::EffectRunner {
            crate::app::effect_runner::EffectRunner::builder()
                .metadata_provider(Arc::new(MockMetadataProvider::new()))
                .query_executor(Arc::new(MockQueryExecutor::new()))
                .dsn_builder(Arc::new(FakeDsnBuilder))
                .er_exporter(Arc::new(NoopErExporter))
                .config_writer(Arc::new(NoopConfigWriter))
                .er_log_writer(Arc::new(NoopErLogWriter))
                .connection_store(Arc::new(MockConnectionStore::new()))
                .service_file_reader(Arc::new(NoopServiceFileReader))
                .clipboard(Arc::new(NoopClipboardWriter))
                .folder_opener(Arc::new(NoopFolderOpener))
                .metadata_cache(TtlCache::new(60))
                .action_tx(action_tx)
                .build()
        }

        #[tokio::test]
        async fn dispatches_action_with_built_dsn() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_runner_with_dsn_builder(tx);

            let profile = ConnectionProfile::new(
                "My DB",
                "db.example.com",
                5432,
                "mydb",
                "user",
                "pass",
                SslMode::Prefer,
            )
            .unwrap();

            let mut state = AppState::new("test".to_string());
            state.set_connections(vec![profile]);

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::SwitchConnection {
                        connection_index: 0,
                    }],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let action = rx.recv().await.expect("action dispatched");
            match action {
                Action::SwitchConnection(ConnectionTarget { id, dsn, name }) => {
                    assert_eq!(dsn, "fake://db.example.com:5432/mydb");
                    assert_eq!(name, "My DB");
                    assert_eq!(id, state.connections()[0].id);
                }
                other => panic!("expected SwitchConnection, got {:?}", other),
            }
        }

        #[tokio::test]
        async fn out_of_bounds_index_is_noop() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_runner_with_dsn_builder(tx);

            let mut state = AppState::new("test".to_string());

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::SwitchConnection {
                        connection_index: 99,
                    }],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(rx.try_recv().is_err());
        }
    }
}
