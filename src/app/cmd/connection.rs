use std::sync::Arc;

use crate::cmd::runner::ConnectionDeps;
use crate::cmd::sqlite_path_validate::{
    canonicalize_sqlite_database_path, validate_sqlite_database_path,
};
use crate::domain::SqlitePathError;
use crate::domain::connection::{
    ConnectionConfig, ConnectionEndpoint, ConnectionId, ConnectionProfile, ConnectionProfileError,
    DatabaseType, SqliteConnectionConfig,
};
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::{
    ConnectionStore, ConnectionStoreError, MetadataProvider, MySqlConnectionProbe,
    PgServiceEntryReader, ServiceFileError, SqlitePathValidator,
};
use crate::update::action::{
    Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload, SavedConnection,
};

fn save_if_active<T>(
    run_guard: &ConnectionSaveGuard,
    run_id: u64,
    save: impl FnOnce() -> T,
) -> Option<T> {
    if !run_guard.begin_persistence(run_id) {
        return None;
    }
    let result = save();
    run_guard.finish_save(run_id);
    Some(result)
}

pub(in crate::cmd) struct SaveAndConnectRequest {
    pub(in crate::cmd) id: Option<ConnectionId>,
    pub(in crate::cmd) name: String,
    pub(in crate::cmd) config: ConnectionConfig,
    pub(in crate::cmd) run_id: u64,
    pub(in crate::cmd) run_guard: Arc<ConnectionSaveGuard>,
}

pub(in crate::cmd) async fn save_and_connect(
    connection: ConnectionDeps,
    metadata_provider: Arc<dyn MetadataProvider>,
    request: SaveAndConnectRequest,
) -> Option<Action> {
    let SaveAndConnectRequest {
        id,
        name,
        config,
        run_id,
        run_guard,
    } = request;
    let outcome = async {
        let id = id.unwrap_or_else(ConnectionId::new);
        let mut profile = ConnectionProfile::with_id_and_config(id, name, config)?;
        let mut mysql_lower_case_table_names = None;
        let mut metadata = None;
        match profile.database_type() {
            DatabaseType::SQLite => {
                profile =
                    normalize_sqlite_profile(profile, &connection.sqlite_path_validator).await?;
            }
            DatabaseType::MySQL => {
                let endpoint = ConnectionEndpoint::from(&profile.config);
                let probe = connection
                    .mysql_connection_probe
                    .probe(&endpoint)
                    .await
                    .map_err(|error| ConnectionSaveError::Probe { error })?;
                mysql_lower_case_table_names = Some(probe.lower_case_table_names);
            }
            DatabaseType::PostgreSQL => {
                let endpoint = ConnectionEndpoint::from(&profile.config);
                metadata = Some(Arc::new(metadata_provider.fetch_metadata(&endpoint).await?));
            }
        }
        let target = ConnectionTarget::from_profile(&profile);
        let store = Arc::clone(&connection.connection_store);
        let saved = tokio::task::spawn_blocking(move || {
            save_if_active(&run_guard, run_id, || store.save(&profile))
        })
        .await
        .expect("connection store save task panicked");
        match saved {
            None => Ok(None),
            Some(Err(error)) => Err(ConnectionSaveError::Store(error.to_string())),
            Some(Ok(())) => Ok(Some(SavedConnection {
                target,
                mysql_lower_case_table_names,
                metadata,
            })),
        }
    }
    .await;
    outcome
        .transpose()
        .map(|outcome| Action::ConnectionSaved { run_id, outcome })
}

pub(in crate::cmd) async fn probe_mysql_connection(
    probe: Arc<dyn MySqlConnectionProbe>,
    target: ConnectionTarget,
    run_id: u64,
) -> Action {
    let outcome = probe
        .probe(&target.endpoint)
        .await
        .map(|probe| probe.lower_case_table_names);
    Action::MySqlConnectionProbed {
        target,
        run_id,
        outcome,
    }
}

pub(in crate::cmd) async fn load_connection_for_edit(
    store: Arc<dyn ConnectionStore>,
    id: ConnectionId,
) -> Action {
    let outcome = run_blocking(move || match store.find_by_id(&id) {
        Ok(Some(profile)) => Ok(Box::new(profile)),
        Ok(None) => Err(ConnectionStoreError::NotFound(id.to_string()).to_string()),
        Err(error) => Err(error.to_string()),
    })
    .await;
    Action::ConnectionEditLoaded(outcome)
}

pub(in crate::cmd) async fn load_connections(
    store: Arc<dyn ConnectionStore>,
    reader: Arc<dyn PgServiceEntryReader>,
) -> Action {
    run_blocking(move || {
        let (profiles, profile_load_warning) = match store.load_all() {
            Ok(p) => (p, None),
            Err(e) => (vec![], Some(e.to_string())),
        };
        let (services, service_file_path, service_load_warning) = match reader.read_services() {
            Ok((s, p)) => (s, Some(p), None),
            Err(ServiceFileError::NotFound(_)) => (vec![], None, None),
            Err(e) => (vec![], None, Some(e.to_string())),
        };

        Action::ConnectionsLoaded(ConnectionsLoadedPayload {
            profiles,
            services,
            service_file_path,
            profile_load_warning,
            service_load_warning,
        })
    })
    .await
}

pub(in crate::cmd) async fn delete_connection(
    store: Arc<dyn ConnectionStore>,
    id: ConnectionId,
) -> Action {
    let outcome = run_blocking(move || store.delete(&id).map(|()| id))
        .await
        .map_err(|error| error.to_string());
    Action::ConnectionDeleted(outcome)
}

async fn run_blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work)
        .await
        .expect("connection store task panicked")
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
    use std::sync::Arc;
    use tokio::sync::mpsc;

    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures::{self, NoopRenderer};
    use crate::domain::DatabaseMetadata;
    use crate::domain::connection::{
        ConnectionConfig, ConnectionEndpoint, ConnectionId, ConnectionProfileError, DatabaseType,
        MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig, SqliteConnectionConfig,
        SqlitePathError, SslMode,
    };
    use crate::model::app_state::AppState;
    use crate::ports::outbound::connection_store::MockConnectionStore;
    use crate::ports::outbound::metadata::MockMetadataProvider;
    use crate::ports::outbound::mysql_connection_probe::MockMySqlConnectionProbe;
    use crate::ports::outbound::query_executor::MockQueryExecutor;
    use crate::ports::outbound::{
        ConnectionStoreError, DbOperationError, MySqlConnectionProbeResult,
    };
    use crate::update::action::{
        Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload,
    };

    mod save_connection {
        use super::super::save_if_active;
        use super::*;
        use crate::update::action::SavedConnection;
        use mockall::predicate::eq;
        use std::fs;
        use tempfile::tempdir;

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
            let endpoint = ConnectionEndpoint::from(&mysql_config(Some("app")));
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(endpoint))
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
                    profile.config.as_mysql().unwrap().database.as_deref(),
                    Some("app")
                );
                Ok(())
            });

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::ConnectionSaved { run_id: 1, outcome: Ok(SavedConnection { target: ConnectionTarget { endpoint, .. }, .. }), .. } if endpoint.engine() == DatabaseType::MySQL
                    && endpoint.selected_database() == Some("app")
            ));
        }

        #[tokio::test]
        async fn mysql_profile_is_not_saved_when_probe_fails() {
            let endpoint = ConnectionEndpoint::from(&mysql_config(None));
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(endpoint))
                .once()
                .returning(|_| {
                    Err(DbOperationError::ConnectionFailed(
                        "access denied for user".to_string(),
                    ))
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(matches!(
                action,
                Action::ConnectionSaved {
                    run_id: 1,
                    outcome: Err(ConnectionSaveError::Probe { .. })
                }
            ));
        }

        #[tokio::test]
        async fn mysql_profile_is_not_saved_when_run_is_cancelled_after_probe() {
            let endpoint = ConnectionEndpoint::from(&mysql_config(Some("app")));
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let guard_for_probe = Arc::clone(&run_guard);
            let mut probe = MockMySqlConnectionProbe::new();
            probe
                .expect_probe()
                .with(eq(endpoint))
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
            let runner = test_fixtures::make_runner_with_probe(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
                Arc::new(probe),
            );
            let mut renderer = NoopRenderer;
            let mut state = AppState::new("test".to_string());

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
            let endpoint = ConnectionEndpoint::from(&postgres_config());
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let guard_for_provider = Arc::clone(&run_guard);
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(endpoint))
                .once()
                .returning(move |_| {
                    guard_for_provider.cancel();
                    Ok(DatabaseMetadata::new("app".to_string()))
                });

            let mut store = MockConnectionStore::new();
            store.expect_save().never();
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
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
            let endpoint = ConnectionEndpoint::from(&postgres_config());
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(endpoint))
                .once()
                .returning(|_| Ok(DatabaseMetadata::new("app".to_string())));

            let mut store = MockConnectionStore::new();
            store.expect_save().once().returning(|_| Ok(()));
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            assert!(matches!(
                run.actions.into_iter().next(),
                Some(Action::ConnectionSaved { run_id: 1, outcome: Ok(SavedConnection { metadata: Some(metadata), .. }), .. }) if metadata.database_name == "app"
            ));
        }

        #[tokio::test]
        async fn postgres_store_failure_does_not_emit_save_completion() {
            let endpoint = ConnectionEndpoint::from(&postgres_config());
            let mut provider = MockMetadataProvider::new();
            provider
                .expect_fetch_metadata()
                .with(eq(endpoint))
                .once()
                .returning(|_| Ok(DatabaseMetadata::new("app".to_string())));

            let mut store = MockConnectionStore::new();
            store.expect_save().once().returning(|_| {
                Err(ConnectionStoreError::Io(Arc::new(std::io::Error::other(
                    "save failed",
                ))))
            });
            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            assert!(matches!(
                run.actions.into_iter().next(),
                Some(Action::ConnectionSaved { run_id: 1, outcome: Err(ConnectionSaveError::Store(error)) }) if error == "IO error: save failed"
            ));
        }

        #[test]
        fn cancelled_save_does_not_start_persistence() {
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let mut saved = false;

            run_guard.cancel();
            let result = save_if_active(&run_guard, 1, || saved = true);

            assert!(result.is_none());
            assert!(!saved);
        }

        #[test]
        fn finishing_cancelled_save_does_not_clear_new_run() {
            let run_guard = test_fixtures::active_connection_save_guard(1);

            let result = save_if_active(&run_guard, 1, || {
                run_guard.cancel();
                run_guard.arm_save(2);
            });

            assert!(result.is_some());
            assert!(save_if_active(&run_guard, 2, || ()).is_some());
        }

        #[test]
        fn stale_save_does_not_consume_new_run() {
            let run_guard = test_fixtures::active_connection_save_guard(1);
            let mut saved = false;

            run_guard.arm_save(2);
            let result = save_if_active(&run_guard, 1, || saved = true);

            assert!(result.is_none());
            assert!(!saved);
            assert!(save_if_active(&run_guard, 2, || ()).is_some());
        }

        #[test]
        fn persistence_starts_only_once_per_run() {
            let run_guard = test_fixtures::active_connection_save_guard(1);

            let result = save_if_active(&run_guard, 1, || {
                assert!(save_if_active(&run_guard, 1, || ()).is_none());
            });

            assert!(result.is_some());
            assert!(save_if_active(&run_guard, 1, || ()).is_none());
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
            let expected_endpoint = ConnectionEndpoint::SQLite(
                SqliteConnectionConfig::new(expected_path.to_str().unwrap()).unwrap(),
            );

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
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionSaved { run_id: 1, outcome: Ok(SavedConnection { target: ConnectionTarget { ref endpoint, .. }, .. }), .. } if **endpoint == expected_endpoint
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
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(store),
                tx,
            );
            let mut renderer = NoopRenderer;
            let mut state = AppState::new("test".to_string());

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
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionSaved {
                        run_id: 1,
                        outcome: Err(ConnectionSaveError::Validation(
                            ConnectionProfileError::SqlitePath(SqlitePathError::FileNotFound(_)),
                        ))
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(action, Action::ConnectionDeleted(Ok(_))),
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionDeleted(Err(ref error))
                        if error == "Connection not found: id"
                ),
                "expected ConnectionDeleteFailed, got {action:?}"
            );
        }
    }

    mod load_connection_for_edit {
        use super::*;

        #[tokio::test]
        async fn missing_connection_returns_display_error() {
            let mut mock_store = MockConnectionStore::new();
            mock_store
                .expect_find_by_id()
                .once()
                .returning(|_| Ok(None));

            let (tx, mut rx) = mpsc::channel(8);
            let runner = test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(mock_store),
                tx,
            );

            let run = test_fixtures::run_one_effect(
                &runner,
                Effect::LoadConnectionForEdit {
                    id: ConnectionId::from_string("id"),
                },
                AppState::new("test".to_string()),
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            assert!(matches!(
                run.actions.into_iter().next(),
                Some(Action::ConnectionEditLoaded(Err(error)))
                    if error == "Connection not found: id"
            ));
        }
    }

    mod load_connections {
        use super::*;
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
    }
}
