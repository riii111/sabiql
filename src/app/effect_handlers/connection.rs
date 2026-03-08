use std::sync::Arc;

use color_eyre::eyre::Result;

use super::EffectContext;
use crate::app::action::{Action, ConnectionTarget, ConnectionsLoadedPayload};
use crate::app::effect::Effect;
use crate::app::ports::ServiceFileError;
use crate::app::state::AppState;
use crate::domain::connection::ConnectionProfile;

pub(crate) async fn run(
    effect: Effect,
    ctx: &EffectContext<'_>,
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
                            ctx.action_tx
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
                            ctx.action_tx
                                .blocking_send(Action::ConnectionSaveFailed(e.to_string()))
                                .ok();
                            return Ok(());
                        }
                    }
                }
            };
            let id = profile.id.clone();
            let dsn = ctx.dsn_builder.build_dsn(&profile);
            let name = profile.name.as_str().to_string();
            let store = Arc::clone(ctx.connection_store);
            let tx = ctx.action_tx.clone();

            let provider = Arc::clone(ctx.metadata_provider);
            let cache = ctx.metadata_cache.clone();
            tokio::spawn(async move {
                match provider.fetch_metadata(&dsn).await {
                    Ok(metadata) => {
                        cache.set(dsn.clone(), metadata).await;
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
            let store = Arc::clone(ctx.connection_store);
            let tx = ctx.action_tx.clone();

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
            let store = Arc::clone(ctx.connection_store);
            let reader = Arc::clone(ctx.service_file_reader);
            let tx = ctx.action_tx.clone();

            tokio::task::spawn_blocking(move || {
                let profiles = store.load_all().unwrap_or_default();
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
                    service_load_warning,
                }))
                .ok();
            });
            Ok(())
        }

        Effect::DeleteConnection { id } => {
            let store = Arc::clone(ctx.connection_store);
            let tx = ctx.action_tx.clone();

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
                let dsn = ctx.dsn_builder.build_dsn(profile);
                let name = profile.display_name().to_string();
                let id = profile.id.clone();
                ctx.action_tx
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
                ctx.action_tx
                    .send(Action::SwitchConnection(ConnectionTarget { id, dsn, name }))
                    .await
                    .ok();
            }
            Ok(())
        }

        _ => unreachable!("connection::run called with non-connection effect"),
    }
}
