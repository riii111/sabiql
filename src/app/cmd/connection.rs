use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::cmd::effect::Effect;
use crate::cmd::runner::ConnectionDeps;
use crate::cmd::sqlite_path_validate::{
    canonicalize_sqlite_database_path, validate_sqlite_database_path,
};
use crate::domain::SqlitePathError;
use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType,
    SqliteConnectionConfig,
};
use crate::model::app_state::AppState;
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::{
    ConnectionStoreError, MetadataProvider, ServiceFileError, SqlitePathValidator,
};
use crate::update::action::{
    Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload,
};

#[derive(Default)]
pub(crate) struct ConnectionTaskOwner {
    active: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl ConnectionTaskOwner {
    pub(crate) async fn replace<F>(&self, task: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.cancel().await;
        let task = tokio::spawn(task);
        *self.active.lock().expect("connection task lock poisoned") = Some(task);
    }

    pub(crate) async fn cancel(&self) {
        if let Some(task) = self.abort() {
            let _ = task.await;
        }
    }

    pub(crate) fn abort(&self) -> Option<JoinHandle<()>> {
        let task = self
            .active
            .lock()
            .expect("connection task lock poisoned")
            .take();
        if let Some(task) = &task {
            task.abort();
        }
        task
    }
}

impl Drop for ConnectionTaskOwner {
    fn drop(&mut self) {
        if let Some(task) = self
            .active
            .get_mut()
            .expect("connection task lock poisoned")
            .take()
        {
            task.abort();
        }
    }
}

fn claim_and_save<T>(
    run_guard: &ConnectionSaveGuard,
    run_id: u64,
    save: impl FnOnce() -> T,
) -> Option<T> {
    if !run_guard.claim(run_id) || !run_guard.begin_persistence(run_id) {
        return None;
    }
    let result = save();
    run_guard.finish_save(run_id);
    Some(result)
}

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    connection: &ConnectionDeps,
    connection_task: &ConnectionTaskOwner,
    metadata_provider: &Arc<dyn MetadataProvider>,
    state: &AppState,
) {
    match effect {
        Effect::SaveAndConnect {
            id,
            name,
            config,
            run_id,
            run_guard,
        } => {
            let database_type = config.database_type();
            let id = id.unwrap_or_else(ConnectionId::new);
            let profile = ConnectionProfile::with_id_and_config(id, name, config);
            let profile = match profile {
                Ok(p) => p,
                Err(e) => {
                    action_tx
                        .send(Action::ConnectionSaveFailed {
                            error: e.into(),
                            database_type,
                            run_id,
                        })
                        .await
                        .ok();
                    return;
                }
            };
            let store = Arc::clone(&connection.connection_store);
            let tx = action_tx.clone();

            if profile.database_type() == DatabaseType::SQLite {
                let profile = match normalize_sqlite_profile(
                    profile,
                    &connection.sqlite_path_validator,
                )
                .await
                {
                    Ok(profile) => profile,
                    Err(error) => {
                        action_tx
                            .send(Action::ConnectionSaveFailed {
                                error: error.into(),
                                database_type,
                                run_id,
                            })
                            .await
                            .ok();
                        return;
                    }
                };
                let dsn = connection.dsn_builder.build_dsn(&profile);
                let target = ConnectionTarget::from_profile(&profile, dsn);
                let database_type = target.database_type;

                connection_task
                    .replace(async move {
                        tokio::task::spawn_blocking(move || {
                            match claim_and_save(&run_guard, run_id, || store.save(&profile)) {
                                Some(Ok(())) => {
                                    tx.blocking_send(Action::ConnectionSaveCompleted {
                                        target,
                                        run_id,
                                        mysql_lower_case_table_names: None,
                                        metadata: None,
                                    })
                                    .ok();
                                }
                                Some(Err(e)) => {
                                    tx.blocking_send(Action::ConnectionSaveFailed {
                                        error: e.into(),
                                        database_type,
                                        run_id,
                                    })
                                    .ok();
                                }
                                None => {}
                            }
                        })
                        .await
                        .expect("connection store save task panicked");
                    })
                    .await;
                return;
            }

            let dsn = connection.dsn_builder.build_dsn(&profile);
            let database_type = profile.database_type();

            if database_type == DatabaseType::MySQL {
                let target = ConnectionTarget::from_profile(&profile, dsn);
                let probe = Arc::clone(&connection.mysql_connection_probe);
                connection_task
                    .replace(async move {
                        match probe.probe(&target.dsn).await {
                            Ok(probe_result) => {
                                let save_result = tokio::task::spawn_blocking(move || {
                                    claim_and_save(&run_guard, run_id, || store.save(&profile))
                                })
                                .await
                                .expect("connection store save task panicked");
                                match save_result {
                                    Some(Ok(())) => {
                                        tx.send(Action::ConnectionSaveCompleted {
                                            target,
                                            run_id,
                                            mysql_lower_case_table_names: Some(
                                                probe_result.lower_case_table_names,
                                            ),
                                            metadata: None,
                                        })
                                        .await
                                        .ok();
                                    }
                                    Some(Err(e)) => {
                                        tx.send(Action::ConnectionSaveFailed {
                                            error: e.into(),
                                            database_type,
                                            run_id,
                                        })
                                        .await
                                        .ok();
                                    }
                                    None => {}
                                }
                            }
                            Err(e) => {
                                tx.send(Action::ConnectionSaveFailed {
                                    error: ConnectionSaveError::Probe {
                                        error: e,
                                        dsn: target.dsn.clone(),
                                    },
                                    database_type,
                                    run_id,
                                })
                                .await
                                .ok();
                            }
                        }
                    })
                    .await;
                return;
            }

            let target = ConnectionTarget::from_profile(&profile, dsn);
            let provider = Arc::clone(metadata_provider);
            let dsn = target.dsn.clone();
            connection_task
                .replace(async move {
                    match provider.fetch_metadata(&dsn).await {
                        Ok(metadata) => {
                            let save_result = tokio::task::spawn_blocking(move || {
                                claim_and_save(&run_guard, run_id, || store.save(&profile))
                            })
                            .await
                            .expect("connection store save task panicked");
                            match save_result {
                                Some(Ok(())) => {
                                    tx.send(Action::ConnectionSaveCompleted {
                                        target,
                                        run_id,
                                        mysql_lower_case_table_names: None,
                                        metadata: Some(Arc::new(metadata)),
                                    })
                                    .await
                                    .ok();
                                }
                                Some(Err(e)) => {
                                    tx.send(Action::ConnectionSaveFailed {
                                        error: e.into(),
                                        database_type,
                                        run_id,
                                    })
                                    .await
                                    .ok();
                                }
                                None => {}
                            }
                        }
                        Err(e) => {
                            tx.send(Action::ConnectionSaveFailed {
                                error: e.into(),
                                database_type,
                                run_id,
                            })
                            .await
                            .ok();
                        }
                    }
                })
                .await;
        }

        Effect::ProbeMySqlConnection { target, run_id } => {
            let probe = Arc::clone(&connection.mysql_connection_probe);
            let tx = action_tx.clone();
            connection_task
                .replace(async move {
                    match probe.probe(&target.dsn).await {
                        Ok(probe_result) => tx
                            .send(Action::MySqlConnectionProbeCompleted {
                                target,
                                run_id,
                                lower_case_table_names: probe_result.lower_case_table_names,
                            })
                            .await
                            .ok(),
                        Err(error) => tx
                            .send(Action::MySqlConnectionProbeFailed {
                                target,
                                run_id,
                                error,
                            })
                            .await
                            .ok(),
                    };
                })
                .await;
        }

        Effect::LoadConnectionForEdit { id } => {
            let store = Arc::clone(&connection.connection_store);
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
        }

        Effect::LoadConnections => {
            let store = Arc::clone(&connection.connection_store);
            let reader = connection.pg_service_entry_reader.clone();
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
        }

        Effect::DeleteConnection { id } => {
            let store = Arc::clone(&connection.connection_store);
            let tx = action_tx.clone();

            tokio::task::spawn_blocking(move || match store.delete(&id) {
                Ok(()) => {
                    tx.blocking_send(Action::ConnectionDeleted(id)).ok();
                }
                Err(e) => {
                    tx.blocking_send(Action::ConnectionDeleteFailed(e)).ok();
                }
            });
        }

        Effect::SwitchConnection { connection_index } => {
            if let Some(profile) = state.connections().get(connection_index) {
                let dsn = connection.dsn_builder.build_dsn(profile);
                action_tx
                    .send(Action::SwitchConnection(ConnectionTarget::from_profile(
                        profile, dsn,
                    )))
                    .await
                    .ok();
            }
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
                        database: None,
                    }))
                    .await
                    .ok();
            }
        }

        _ => unreachable!("connection::run called with non-connection effect"),
    }
}

async fn normalize_sqlite_profile(
    profile: ConnectionProfile,
    validator: &Arc<dyn SqlitePathValidator>,
) -> Result<ConnectionProfile, ConnectionProfileError> {
    let path = profile
        .sqlite_config()
        .expect("SQLite profile requires SQLite config")
        .path()
        .to_string();
    let canonical_path = canonicalize_sqlite_database_path(validator, path)
        .await
        .map_err(ConnectionProfileError::SqlitePath)?;
    let canonical_path = canonical_path.to_str().ok_or_else(|| {
        ConnectionProfileError::SqlitePath(SqlitePathError::Io(
            "SQLite database path is not valid UTF-8".to_string(),
        ))
    })?;
    validate_sqlite_database_path(validator, canonical_path.to_string())
        .await
        .map_err(ConnectionProfileError::SqlitePath)?;
    let config = SqliteConnectionConfig::new(canonical_path.to_string())?;

    ConnectionProfile::with_id_and_config(
        profile.id.clone(),
        profile.name.as_str(),
        ConnectionConfig::SQLite(config),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    use crate::cmd::completion_engine::CompletionEngine;
    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures::{self, NoopRenderer};
    use crate::domain::DatabaseMetadata;
    use crate::domain::connection::{
        ConnectionConfig, ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType,
        MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig, SqliteConnectionConfig,
        SqlitePathError, SslMode,
    };
    use crate::model::app_state::AppState;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::mysql_connection_probe::MockMySqlConnectionProbe;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{
        ConnectionStoreError, DbOperationError, DsnBuilder, MySqlConnectionProbeResult,
    };
    use crate::services::AppServices;
    use crate::update::action::{
        Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload,
    };

    mod save_connection {
        use super::*;
        use mockall::predicate::eq;
        use std::fs;
        use tempfile::tempdir;

        struct SqliteDsnBuilder;
        impl DsnBuilder for SqliteDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                format!("sqlite://{}", profile.sqlite_config().unwrap().path())
            }
        }

        struct MySqlDsnBuilder;
        impl DsnBuilder for MySqlDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                let config = profile.mysql_config().unwrap();
                let database = config
                    .database
                    .as_deref()
                    .map_or_else(String::new, |database| format!("/{database}"));
                format!(
                    "mysql://{}:secret@{}:{}{}?ssl-mode={}",
                    config.username, config.host, config.port, database, config.ssl_mode
                )
            }
        }

        struct PostgresDsnBuilder;
        impl DsnBuilder for PostgresDsnBuilder {
            fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
                "postgres://localhost/app".to_string()
            }
        }

        fn mysql_config(database: Option<&str>) -> ConnectionConfig {
            ConnectionConfig::MySQL(MySqlConnectionConfig::new(
                "localhost",
                3306,
                database.map(str::to_string),
                "user",
                "secret",
                MySqlSslMode::Required,
            ))
        }

        fn postgres_config() -> ConnectionConfig {
            ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                "localhost",
                5432,
                "app",
                "user",
                "secret",
                SslMode::Prefer,
            ))
        }

        #[tokio::test]
        async fn mysql_profile_is_saved_only_after_probe_success() {
            let dsn = "mysql://user:secret@localhost:3306/app?ssl-mode=REQUIRED";
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(dsn.to_string()))
                .once()
                .returning(|_| {
                    Ok(MySqlConnectionProbeResult {
                        lower_case_table_names: 0,
                    })
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().once().returning(|profile| {
                assert_eq!(profile.database_type(), DatabaseType::MySQL);
                assert_eq!(
                    profile.mysql_config().unwrap().database.as_deref(),
                    Some("app")
                );
                Ok(())
            });

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(MySqlDsnBuilder),
                Arc::new(probe),
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "MySQL".to_string(),
                    config: mysql_config(Some("app")),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::ConnectionSaveCompleted {
                    target: ConnectionTarget {
                        database: Some(database),
                        database_type: DatabaseType::MySQL,
                        ..
                    },
                    run_id: 1,
                    ..
                } if database == "app"
            ));
        }

        #[tokio::test]
        async fn mysql_profile_is_not_saved_when_probe_fails() {
            let dsn = "mysql://user:secret@localhost:3306?ssl-mode=REQUIRED";
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(dsn.to_string()))
                .once()
                .returning(|_| {
                    Err(DbOperationError::ConnectionFailed(
                        "access denied for user".to_string(),
                    ))
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(MySqlDsnBuilder),
                Arc::new(probe),
            );
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "MySQL".to_string(),
                    config: mysql_config(None),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::ConnectionSaveFailed {
                    error: ConnectionSaveError::Probe { dsn, .. },
                    database_type: DatabaseType::MySQL,
                    run_id: 1,
                } if dsn == "mysql://user:secret@localhost:3306?ssl-mode=REQUIRED"
            ));
        }

        #[tokio::test]
        async fn mysql_profile_is_not_saved_when_run_is_cancelled_after_probe() {
            let dsn = "mysql://user:secret@localhost:3306/app?ssl-mode=REQUIRED";
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let guard_for_probe = Arc::clone(&run_guard);
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(dsn.to_string()))
                .once()
                .returning(move |_| {
                    guard_for_probe.cancel();
                    Ok(MySqlConnectionProbeResult {
                        lower_case_table_names: 0,
                    })
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn_and_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(MySqlDsnBuilder),
                Arc::new(probe),
            );
            let mut renderer = NoopRenderer;
            let mut state = AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());

            runner
                .execute_effects(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "MySQL".to_string(),
                        config: mysql_config(Some("app")),
                        run_id: 1,
                        run_guard,
                    }],
                    &mut renderer,
                    &mut state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                    .await
                    .is_err()
            );
        }

        #[tokio::test]
        async fn postgres_validated_metadata_is_not_sent_when_save_is_cancelled() {
            let dsn = "postgres://localhost/app".to_string();
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let guard_for_provider = Arc::clone(&run_guard);
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(dsn.clone()))
                .once()
                .returning(move |_| {
                    guard_for_provider.cancel();
                    Ok(DatabaseMetadata::new("app".to_string()))
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(PostgresDsnBuilder),
            );

            runner
                .execute_effects(
                    vec![Effect::SaveAndConnect {
                        id: None,
                        name: "PostgreSQL".to_string(),
                        config: postgres_config(),
                        run_id: 1,
                        run_guard,
                    }],
                    &mut NoopRenderer,
                    &mut AppState::new("test".to_string()),
                    &RefCell::new(CompletionEngine::new()),
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                    .await
                    .is_err()
            );
        }

        #[tokio::test]
        async fn postgres_validated_metadata_is_carried_by_save_action() {
            let dsn = "postgres://localhost/app".to_string();
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(dsn.clone()))
                .once()
                .returning(|_| Ok(DatabaseMetadata::new("app".to_string())));

            let mut store = MockConnectionStore::new();
            store.expect_save().once().returning(|_| Ok(()));
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(PostgresDsnBuilder),
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "PostgreSQL".to_string(),
                    config: postgres_config(),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            assert!(matches!(
                run.actions.into_iter().next(),
                Some(Action::ConnectionSaveCompleted {
                    run_id: 1,
                    metadata: Some(metadata),
                    ..
                }) if metadata.database_name == "app"
            ));
        }

        #[tokio::test]
        async fn postgres_store_failure_does_not_emit_save_completion() {
            let dsn = "postgres://localhost/app".to_string();
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(dsn.clone()))
                .once()
                .returning(|_| Ok(DatabaseMetadata::new("app".to_string())));

            let mut store = MockConnectionStore::new();
            store.expect_save().once().returning(|_| {
                Err(ConnectionStoreError::Io(Arc::new(std::io::Error::other(
                    "save failed",
                ))))
            });
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(PostgresDsnBuilder),
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "PostgreSQL".to_string(),
                    config: postgres_config(),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            assert!(matches!(
                run.actions.into_iter().next(),
                Some(Action::ConnectionSaveFailed {
                    database_type: DatabaseType::PostgreSQL,
                    run_id: 1,
                    ..
                })
            ));
        }

        #[test]
        fn cancel_after_claim_prevents_save_from_starting() {
            let run_guard = test_fixtures::active_connection_save_guard(1);

            assert!(run_guard.claim(1));

            run_guard.cancel();
            assert!(!run_guard.begin_persistence(1));
        }

        #[test]
        fn finishing_cancelled_save_does_not_clear_new_run() {
            let run_guard = test_fixtures::active_connection_save_guard(1);

            assert!(run_guard.claim(1));
            assert!(run_guard.begin_persistence(1));

            run_guard.cancel();
            run_guard.arm_save(2);
            run_guard.finish_save(1);

            assert!(run_guard.claim(2));
        }

        #[tokio::test]
        async fn sqlite_profile_is_canonicalized_before_save() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();
            let input_path = dir.path().join("nested").join("..").join("app.db");
            fs::create_dir(dir.path().join("nested")).unwrap();
            let input_path = input_path.to_str().unwrap().to_string();
            let expected_path = fs::canonicalize(&path).unwrap();
            let expected_dsn = format!("sqlite://{}", expected_path.display());

            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_save().once().returning(move |profile| {
                assert_eq!(profile.database_type(), DatabaseType::SQLite);
                assert_eq!(
                    profile.sqlite_config().unwrap().path(),
                    expected_path.to_str().unwrap()
                );
                Ok(())
            });

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
                Arc::new(SqliteDsnBuilder),
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "Local".to_string(),
                    config: ConnectionConfig::SQLite(
                        SqliteConnectionConfig::new(input_path).unwrap(),
                    ),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionSaveCompleted {
                        target: ConnectionTarget { ref dsn, .. },
                        run_id: 1,
                        ..
                    } if dsn == &expected_dsn
                ),
                "expected sqlite ConnectionSaveCompleted, got {action:?}"
            );
        }

        #[tokio::test]
        async fn sqlite_profile_is_not_saved_when_run_is_cancelled() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("app.db");
            fs::write(&path, b"").unwrap();
            let path = path.to_str().unwrap().to_string();
            let run_guard = test_fixtures::active_connection_save_guard(1);
            run_guard.cancel();

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(SqliteDsnBuilder),
            );
            let mut renderer = NoopRenderer;
            let mut state = AppState::new("test".to_string());
            let completion_engine = RefCell::new(CompletionEngine::new());

            runner
                .execute_effects(
                    vec![
                        Effect::SaveAndConnect {
                            id: None,
                            name: "Local".to_string(),
                            config: ConnectionConfig::SQLite(
                                SqliteConnectionConfig::new(path).unwrap(),
                            ),
                            run_id: 1,
                            run_guard,
                        },
                        Effect::CancelConnectionTask,
                    ],
                    &mut renderer,
                    &mut state,
                    &completion_engine,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                    .await
                    .is_err()
            );
        }

        #[tokio::test]
        async fn sqlite_missing_file_is_rejected_before_save() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let path_str = path.to_str().unwrap().to_string();

            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_save().never();

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_dsn(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
                Arc::new(SqliteDsnBuilder),
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::SaveAndConnect {
                    id: None,
                    name: "Local".to_string(),
                    config: ConnectionConfig::SQLite(
                        SqliteConnectionConfig::new(path_str).unwrap(),
                    ),
                    run_id: 1,
                    run_guard: test_fixtures::active_connection_save_guard(1),
                },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionSaveFailed {
                        error: ConnectionSaveError::Validation(ConnectionProfileError::SqlitePath(
                            SqlitePathError::FileNotFound(_)
                        ),),
                        database_type: DatabaseType::SQLite,
                        run_id: 1,
                    }
                ),
                "expected sqlite path validation failure, got {action:?}"
            );
        }
    }

    mod delete_connection {
        use super::*;

        #[tokio::test]
        async fn success_returns_connection_deleted() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_delete().once().returning(|_| Ok(()));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
            );

            let id = ConnectionId::new();
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::DeleteConnection { id: id.clone() },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
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

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
            );

            let id = ConnectionId::new();
            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::DeleteConnection { id },
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::ConnectionDeleteFailed(_)),
                "expected ConnectionDeleteFailed, got {action:?}"
            );
        }
    }

    mod load_connections {
        use super::*;
        use crate::cmd::runner::{ConnectionDeps, EffectRunner, ErDeps, QueryDeps, UtilityDeps};

        #[tokio::test]
        async fn error_returns_empty_connections_list() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_load_all().once().returning(|| {
                Err(ConnectionStoreError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                ))))
            });

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::LoadConnections,
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::ConnectionsLoaded(ConnectionsLoadedPayload { ref profiles, .. }) if profiles.is_empty()),
                "expected ConnectionsLoaded with empty profiles, got {action:?}"
            );
        }

        #[tokio::test]
        async fn missing_pg_service_reader_skips_service_loading() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_load_all().once().returning(|| Ok(vec![]));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = EffectRunner::new(
                Arc::new(MockMetadataProvider::new()),
                ConnectionDeps {
                    dsn_builder: Arc::new(test_fixtures::NoopDsnBuilder),
                    mysql_connection_probe: Arc::new(test_fixtures::NoopMySqlConnectionProbe),
                    connection_store: Arc::new(mock_store),
                    pg_service_entry_reader: None,
                    sqlite_path_validator: Arc::new(test_fixtures::TestFsSqlitePathValidator),
                },
                QueryDeps {
                    query_executor: Arc::new(MockQueryExecutor::new()),
                    query_history_store: Arc::new(test_fixtures::NoopQueryHistoryStore),
                    sqlite_diagnostics: Arc::new(test_fixtures::NoopSqliteDiagnosticsProvider),
                    cached_result_exporter: Arc::new(test_fixtures::TestCachedResultExporter),
                },
                ErDeps {
                    er_exporter: Arc::new(test_fixtures::NoopErExporter),
                    config_writer: Arc::new(test_fixtures::NoopConfigWriter),
                    er_log_writer: Arc::new(test_fixtures::NoopErLogWriter),
                },
                UtilityDeps {
                    clipboard: Arc::new(test_fixtures::NoopClipboardWriter),
                    folder_opener: Arc::new(test_fixtures::NoopFolderOpener),
                },
                Arc::new(test_fixtures::NoopSettingsStore),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::LoadConnections,
                AppState::new("test".to_string()),
                RefCell::new(CompletionEngine::new()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
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
            test_fixtures::make_runner_with_dsn(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
                Arc::new(FakeDsnBuilder),
            )
        }

        #[tokio::test]
        async fn dispatches_action_with_built_dsn() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_runner_with_dsn_builder(tx);

            let profile = ConnectionProfile::new_postgres(
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

            let mut renderer = NoopRenderer;
            let ce = RefCell::new(CompletionEngine::new());
            runner
                .execute_effects(
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
                    ..
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
            let mut renderer = NoopRenderer;
            let ce = RefCell::new(CompletionEngine::new());
            runner
                .execute_effects(
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
