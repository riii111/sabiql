use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::cmd::runner::ConnectionDeps;
use crate::cmd::single_task_owner::SingleTaskOwner;
use crate::cmd::sqlite_path_validate::{
    canonicalize_sqlite_database_path, validate_sqlite_database_path,
};
use crate::domain::SqlitePathError;
use crate::domain::connection::{
    ConnectionConfig, ConnectionEndpoint, ConnectionId, ConnectionProfile, ConnectionProfileError,
    DatabaseType, SqliteConnectionConfig,
};
use crate::model::app_state::AppState;
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::{
    ConnectionStoreError, MetadataProvider, ServiceFileError, SqlitePathValidator,
};
use crate::update::action::{
    Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload,
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

pub(in crate::cmd) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    connection: &ConnectionDeps,
    connection_task: &SingleTaskOwner,
    metadata_provider: &Arc<dyn MetadataProvider>,
    state: &AppState,
) -> Option<Action> {
    match effect {
        Effect::SaveAndConnect {
            id,
            name,
            config,
            run_id,
            run_guard,
        } => {
            let id = id.unwrap_or_else(ConnectionId::new);
            let profile = ConnectionProfile::with_id_and_config(id, name, config);
            let profile = match profile {
                Ok(p) => p,
                Err(e) => {
                    action_tx
                        .send(Action::ConnectionSaveFailed {
                            error: e.into(),
                            run_id,
                        })
                        .await
                        .ok();
                    return None;
                }
            };
            let store = Arc::clone(&connection.connection_store);
            let tx = action_tx.clone();

            match profile.database_type() {
                DatabaseType::SQLite => {
                    let profile =
                        match normalize_sqlite_profile(profile, &connection.sqlite_path_validator)
                            .await
                        {
                            Ok(profile) => profile,
                            Err(error) => {
                                action_tx
                                    .send(Action::ConnectionSaveFailed {
                                        error: error.into(),
                                        run_id,
                                    })
                                    .await
                                    .ok();
                                return None;
                            }
                        };
                    let target = ConnectionTarget::from_profile(&profile);

                    connection_task
                        .replace(async move {
                            tokio::task::spawn_blocking(move || {
                                match save_if_active(&run_guard, run_id, || store.save(&profile)) {
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
                                            error: ConnectionSaveError::Store(e.to_string()),
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
                }
                DatabaseType::MySQL => {
                    let target = ConnectionTarget::from_profile(&profile);
                    let probe = Arc::clone(&connection.mysql_connection_probe);
                    connection_task
                        .replace(async move {
                            match probe.probe(&target.endpoint).await {
                                Ok(probe_result) => {
                                    let save_result = tokio::task::spawn_blocking(move || {
                                        save_if_active(&run_guard, run_id, || store.save(&profile))
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
                                                error: ConnectionSaveError::Store(e.to_string()),
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
                                        error: ConnectionSaveError::Probe { error: e },
                                        run_id,
                                    })
                                    .await
                                    .ok();
                                }
                            }
                        })
                        .await;
                }
                DatabaseType::PostgreSQL => {
                    let target = ConnectionTarget::from_profile(&profile);
                    let provider = Arc::clone(metadata_provider);
                    connection_task
                        .replace(async move {
                            match provider.fetch_metadata(&target.endpoint).await {
                                Ok(metadata) => {
                                    let save_result = tokio::task::spawn_blocking(move || {
                                        save_if_active(&run_guard, run_id, || store.save(&profile))
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
                                                error: ConnectionSaveError::Store(e.to_string()),
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
                                        run_id,
                                    })
                                    .await
                                    .ok();
                                }
                            }
                        })
                        .await;
                }
            }
            None
        }

        Effect::ProbeMySqlConnection { target, run_id } => {
            let probe = Arc::clone(&connection.mysql_connection_probe);
            let tx = action_tx.clone();
            connection_task
                .replace(async move {
                    match probe.probe(&target.endpoint).await {
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
            None
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
                        ConnectionStoreError::NotFound(id.to_string()).to_string(),
                    ))
                    .ok();
                }
                Err(e) => {
                    tx.blocking_send(Action::ConnectionEditLoadFailed(e.to_string()))
                        .ok();
                }
            });
            None
        }

        Effect::LoadConnections => {
            let store = Arc::clone(&connection.connection_store);
            let reader = Arc::clone(&connection.pg_service_entry_reader);
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
            None
        }

        Effect::DeleteConnection { id } => {
            let store = Arc::clone(&connection.connection_store);
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
            None
        }

        Effect::SwitchConnection { connection_index } => state
            .connections()
            .get(connection_index)
            .map(|profile| Action::SwitchConnection(ConnectionTarget::from_profile(profile))),

        Effect::SwitchToService { service_index } => {
            state.service_entries().get(service_index).map(|entry| {
                Action::SwitchConnection(ConnectionTarget {
                    id: entry.connection_id(),
                    name: entry.display_name().to_owned(),
                    endpoint: Arc::new(ConnectionEndpoint::from(entry)),
                })
            })
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
    use std::sync::Arc;
    use tokio::sync::mpsc;

    use crate::cmd::effect::Effect;
    use crate::cmd::test_fixtures::{self, NoopRenderer};
    use crate::domain::DatabaseMetadata;
    use crate::domain::connection::{
        ConnectionConfig, ConnectionEndpoint, ConnectionId, ConnectionProfile,
        ConnectionProfileError, DatabaseType, MySqlConnectionConfig, MySqlSslMode,
        PostgresConnectionConfig, PostgresEndpoint, ServiceEntry, SqliteConnectionConfig,
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
    use crate::services::AppServices;
    use crate::update::action::{
        Action, ConnectionSaveError, ConnectionTarget, ConnectionsLoadedPayload,
    };

    mod save_connection {
        use super::super::save_if_active;
        use super::*;
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
                    profile.mysql_config().unwrap().database.as_deref(),
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
                Action::ConnectionSaveCompleted {
                    target: ConnectionTarget { endpoint, .. },
                    run_id: 1,
                    ..
                } if endpoint.engine() == DatabaseType::MySQL
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
                Action::ConnectionSaveFailed {
                    error: ConnectionSaveError::Probe { .. },
                    run_id: 1,
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
                Some(Action::ConnectionSaveCompleted {
                    run_id: 1,
                    metadata: Some(metadata),
                    ..
                }) if metadata.database_name == "app"
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
                Some(Action::ConnectionSaveFailed {
                    error: ConnectionSaveError::Store(error),
                    run_id: 1,
                }) if error == "IO error: save failed"
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
                    Action::ConnectionSaveCompleted {
                        target: ConnectionTarget { ref endpoint, .. },
                        run_id: 1,
                        ..
                    } if **endpoint == expected_endpoint
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
                    Action::ConnectionSaveFailed {
                        error: ConnectionSaveError::Validation(ConnectionProfileError::SqlitePath(
                            SqlitePathError::FileNotFound(_)
                        ),),
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
                &mut rx,
                Some(std::time::Duration::from_millis(500)),
            )
            .await
            .unwrap();

            let action = run.actions.into_iter().next().expect("action dispatched");
            assert!(
                matches!(
                    action,
                    Action::ConnectionDeleteFailed(ref error)
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
                Some(Action::ConnectionEditLoadFailed(error))
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

    mod switch_connection {
        use super::*;
        use crate::cmd::runner::EffectRunner;

        fn make_switch_runner(action_tx: mpsc::Sender<Action>) -> EffectRunner {
            test_fixtures::make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                action_tx,
            )
        }

        #[tokio::test]
        async fn dispatches_action_with_built_dsn() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_switch_runner(tx);

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
            let actions = runner
                .execute_effects(
                    vec![Effect::SwitchConnection {
                        connection_index: 0,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(rx.try_recv().is_err());
            let action = actions.into_iter().next().expect("action dispatched");
            match action {
                Action::SwitchConnection(ConnectionTarget { id, name, endpoint }) => {
                    assert_eq!(
                        *endpoint,
                        ConnectionEndpoint::from(&state.connections()[0].config)
                    );
                    assert_eq!(name, "My DB");
                    assert_eq!(id, state.connections()[0].id);
                    assert_eq!(endpoint.engine(), DatabaseType::PostgreSQL);
                }
                other => panic!("expected SwitchConnection, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn out_of_bounds_index_is_noop() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_switch_runner(tx);

            let mut state = AppState::new("test".to_string());
            let mut renderer = NoopRenderer;
            let actions = runner
                .execute_effects(
                    vec![Effect::SwitchConnection {
                        connection_index: 99,
                    }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(actions.is_empty());
            assert!(rx.try_recv().is_err());
        }

        #[tokio::test]
        async fn dispatches_service_action_without_channel() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_switch_runner(tx);

            let mut state = AppState::new("test".to_string());
            state.set_service_entries(vec![ServiceEntry {
                service_name: "analytics".to_string(),
            }]);
            let expected_id = state.service_entries()[0].connection_id();
            let mut renderer = NoopRenderer;
            let actions = runner
                .execute_effects(
                    vec![Effect::SwitchToService { service_index: 0 }],
                    &mut renderer,
                    &mut state,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            assert!(rx.try_recv().is_err());
            assert!(matches!(
                actions.as_slice(),
                [Action::SwitchConnection(ConnectionTarget { id, name, endpoint })]
                    if *id == expected_id
                        && **endpoint
                            == ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Service(
                                "analytics".to_string()
                            ))
                        && name == "analytics"
            ));
        }
    }
}
