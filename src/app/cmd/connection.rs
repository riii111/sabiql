use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::cmd::cache::TtlCache;
use crate::cmd::effect::Effect;
use crate::domain::DatabaseMetadata;
use crate::domain::connection::{ConnectionId, ConnectionProfile, DatabaseType};
use crate::model::app_state::AppState;
use crate::ports::outbound::{
    ConnectionStore, ConnectionStoreError, DsnBuilder, MetadataProvider, PgServiceEntryReader,
    ServiceFileError,
};
use crate::update::action::{Action, ConnectionTarget, ConnectionsLoadedPayload};

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    dsn_builder: &Arc<dyn DsnBuilder>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, Arc<DatabaseMetadata>>,
    connection_store: &Arc<dyn ConnectionStore>,
    pg_service_entry_reader: Option<&Arc<dyn PgServiceEntryReader>>,
    state: &AppState,
) -> Result<()> {
    match effect {
        Effect::SaveAndConnect { id, name, config } => {
            let id = id.unwrap_or_else(ConnectionId::new);
            let profile = ConnectionProfile::with_id_and_config(id, name, config);
            let profile = match profile {
                Ok(p) => p,
                Err(e) => {
                    action_tx
                        .send(Action::ConnectionSaveFailed(e.into()))
                        .await
                        .ok();
                    return Ok(());
                }
            };
            let id = profile.id.clone();
            let dsn = dsn_builder.build_dsn(&profile);
            let name = profile.name.as_str().to_string();
            let database_type = profile.database_type();
            let store = Arc::clone(connection_store);
            let tx = action_tx.clone();

            if profile.database_type() == DatabaseType::SQLite {
                tokio::task::spawn_blocking(move || match store.save(&profile) {
                    Ok(()) => {
                        tx.blocking_send(Action::ConnectionSaveCompleted(ConnectionTarget {
                            id,
                            dsn,
                            name,
                            database_type,
                        }))
                        .ok();
                    }
                    Err(e) => {
                        tx.blocking_send(Action::ConnectionSaveFailed(e.into()))
                            .ok();
                    }
                });
                return Ok(());
            }

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
                                    dsn: dsn.clone(),
                                    name,
                                    database_type,
                                }))
                                .await
                                .ok();
                            }
                            Err(e) => {
                                cache.invalidate(&dsn).await;
                                tx.send(Action::ConnectionSaveFailed(e.into())).await.ok();
                            }
                        }
                    }
                    Err(e) => {
                        tx.send(Action::MetadataFailed(e)).await.ok();
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
                        ConnectionStoreError::NotFound(id.to_string()),
                    ))
                    .ok();
                }
                Err(e) => {
                    tx.blocking_send(Action::ConnectionEditLoadFailed(e)).ok();
                }
            });
            Ok(())
        }

        Effect::LoadConnections => {
            let store = Arc::clone(connection_store);
            let reader = pg_service_entry_reader.cloned();
            let tx = action_tx.clone();

            tokio::task::spawn_blocking(move || {
                let (profiles, profile_load_warning) = match store.load_all() {
                    Ok(p) => (p, None),
                    Err(e) => (vec![], Some(e.to_string())),
                };
                let (services, service_file_path, service_load_warning) =
                    match reader.as_ref().map(|reader| reader.read_services()) {
                        Some(Ok((s, p))) => (s, Some(p), None),
                        Some(Err(ServiceFileError::NotFound(_))) | None => (vec![], None, None),
                        Some(Err(e)) => (vec![], None, Some(e.to_string())),
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
                    tx.blocking_send(Action::ConnectionDeleteFailed(e)).ok();
                }
            });
            Ok(())
        }

        Effect::SwitchConnection { connection_index } => {
            if let Some(profile) = state.connections().get(connection_index) {
                let dsn = dsn_builder.build_dsn(profile);
                let name = profile.display_name().to_string();
                let id = profile.id.clone();
                let database_type = profile.database_type();
                action_tx
                    .send(Action::SwitchConnection(ConnectionTarget {
                        id,
                        dsn,
                        name,
                        database_type,
                    }))
                    .await
                    .ok();
            }
            Ok(())
        }

        Effect::SwitchToService { service_index } => {
            if let Some(entry) = state.service_entries().get(service_index) {
                let id = entry.connection_id();
                let dsn = entry.to_string();
                let name = entry.display_name().to_owned();
                action_tx
                    .send(Action::SwitchConnection(ConnectionTarget {
                        id,
                        dsn,
                        name,
                        database_type: DatabaseType::PostgreSQL,
                    }))
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
    use std::time::Instant;

    use tokio::sync::mpsc;

    use crate::cmd::cache::TtlCache;
    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::effect::Effect;
    use crate::cmd::test_support::*;
    use crate::domain::connection::{
        ConnectionConfig, ConnectionId, ConnectionProfile, DatabaseType, SqliteConnectionConfig,
        SslMode,
    };
    use crate::model::app_state::AppState;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{
        ConnectionStoreError, DsnBuilder, RenderOutput, RenderResult, Renderer,
    };
    use crate::services::AppServices;
    use crate::update::action::{Action, ConnectionTarget, ConnectionsLoadedPayload};

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(
            &mut self,
            _state: &AppState,
            _services: &AppServices,
            _now: Instant,
        ) -> RenderResult<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    mod save_connection {
        use super::*;

        struct SqliteDsnBuilder;
        impl DsnBuilder for SqliteDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                format!("sqlite://{}", profile.sqlite_config().unwrap().path())
            }
        }

        #[tokio::test]
        async fn sqlite_profile_is_saved_before_adapter_metadata_exists() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_save().once().returning(|profile| {
                assert_eq!(profile.database_type(), DatabaseType::SQLite);
                Ok(())
            });

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner_builder(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                cache,
                tx,
            )
            .dsn_builder(Arc::new(SqliteDsnBuilder))
            .build();

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "Local".to_string(),
                        config: ConnectionConfig::SQLite(
                            SqliteConnectionConfig::new("/tmp/app.db").unwrap(),
                        ),
                    }],
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
                matches!(action, Action::ConnectionSaveCompleted(ConnectionTarget { ref dsn, .. }) if dsn == "sqlite:///tmp/app.db"),
                "expected sqlite ConnectionSaveCompleted, got {action:?}"
            );
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
                "expected ConnectionDeleted, got {action:?}"
            );
        }

        #[tokio::test]
        async fn error_returns_connection_delete_failed() {
            let mut mock_store = MockConnectionStore::new();
            mock_store
                .expect_delete()
                .once()
                .returning(|_| Err(ConnectionStoreError::NotFound("id".to_string())));

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
                "expected ConnectionDeleteFailed, got {action:?}"
            );
        }
    }

    mod load_connections {
        use super::*;
        use crate::cmd::runner::EffectRunner;

        #[tokio::test]
        async fn error_returns_empty_connections_list() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_load_all().once().returning(|| {
                Err(ConnectionStoreError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                ))))
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
                "expected ConnectionsLoaded with empty profiles, got {action:?}"
            );
        }

        #[tokio::test]
        async fn missing_pg_service_reader_skips_service_loading() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_load_all().once().returning(|| Ok(vec![]));

            let cache = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = EffectRunner::builder()
                .metadata_provider(Arc::new(MockMetadataProvider::new()))
                .query_executor(Arc::new(MockQueryExecutor::new()))
                .dsn_builder(Arc::new(NoopDsnBuilder))
                .er_exporter(Arc::new(NoopErExporter))
                .config_writer(Arc::new(NoopConfigWriter))
                .er_log_writer(Arc::new(NoopErLogWriter))
                .connection_store(Arc::new(mock_store))
                .clipboard(Arc::new(NoopClipboardWriter))
                .folder_opener(Arc::new(NoopFolderOpener))
                .query_history_store(Arc::new(NoopQueryHistoryStore))
                .metadata_cache(cache)
                .action_tx(tx)
                .build();

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
                matches!(
                    action,
                    Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                        ref services,
                        service_file_path: None,
                        service_load_warning: None,
                        ..
                    }) if services.is_empty()
                ),
                "expected ConnectionsLoaded without services, got {action:?}"
            );
        }
    }

    mod switch_connection {
        use super::*;
        use crate::cmd::runner::EffectRunner;

        struct FakeDsnBuilder;
        impl DsnBuilder for FakeDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                let config = profile.postgres_config().unwrap();
                format!("fake://{}:{}/{}", config.host, config.port, config.database)
            }
        }

        fn make_runner_with_dsn_builder(action_tx: mpsc::Sender<Action>) -> EffectRunner {
            make_runner_builder(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(60),
                action_tx,
            )
            .dsn_builder(Arc::new(FakeDsnBuilder))
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
                Action::SwitchConnection(ConnectionTarget {
                    id,
                    dsn,
                    name,
                    database_type,
                }) => {
                    assert_eq!(dsn, "fake://db.example.com:5432/mydb");
                    assert_eq!(name, "My DB");
                    assert_eq!(id, state.connections()[0].id);
                    assert_eq!(database_type, DatabaseType::PostgreSQL);
                }
                other => panic!("expected SwitchConnection, got {other:?}"),
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
