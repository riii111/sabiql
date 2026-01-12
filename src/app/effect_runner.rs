//! Executes side effects returned by the reducer.
//!
//! # RefCell Borrow Safety
//!
//! When effects need data from `completion_engine` (a `RefCell`), the borrow
//! MUST be dropped before any await point:
//!
//! ```ignore
//! let tables = {
//!     let engine = completion_engine.borrow();
//!     engine.table_details_iter().map(|...| ...).collect()
//! };  // borrow dropped here
//! some_async_operation(tables).await;  // safe
//! ```

use std::cell::RefCell;
use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::action::Action;
use crate::app::cache::TtlCache;
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::er_task::{spawn_er_diagram_task, write_er_failure_log_blocking};
use crate::app::ports::{
    ConfigWriter, ConnectionStore, ErDiagramExporter, MetadataProvider, QueryExecutor, Renderer,
};
use crate::app::state::AppState;
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, ErTableInfo};

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    er_exporter: Arc<dyn ErDiagramExporter>,
    config_writer: Arc<dyn ConfigWriter>,
    connection_store: Arc<dyn ConnectionStore>,
    metadata_cache: TtlCache<String, DatabaseMetadata>,
    action_tx: mpsc::Sender<Action>,
}

impl EffectRunner {
    pub fn new(
        metadata_provider: Arc<dyn MetadataProvider>,
        query_executor: Arc<dyn QueryExecutor>,
        er_exporter: Arc<dyn ErDiagramExporter>,
        config_writer: Arc<dyn ConfigWriter>,
        connection_store: Arc<dyn ConnectionStore>,
        metadata_cache: TtlCache<String, DatabaseMetadata>,
        action_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            metadata_provider,
            query_executor,
            er_exporter,
            config_writer,
            connection_store,
            metadata_cache,
            action_tx,
        }
    }

    pub async fn run<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        self.run_single(seq_effect, tui, state, completion_engine)
                            .await?;
                    }
                }
                single_effect => {
                    self.run_single(single_effect, tui, state, completion_engine)
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn run_single<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        self.run_normal(effect, tui, state, completion_engine).await
    }

    async fn run_normal<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
    ) -> Result<()> {
        match effect {
            Effect::Render => {
                let output = tui.draw(state)?;
                if !state.ui.focus_mode {
                    state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
                }
                state.ui.result_viewport_plan = output.result_viewport_plan;
                state.ui.inspector_pane_height = output.inspector_pane_height;
                state.ui.result_pane_height = output.result_pane_height;
                Ok(())
            }

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
                        // Edit mode: use existing ID
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
                                let _ = self
                                    .action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()));
                                return Ok(());
                            }
                        }
                    }
                    None => {
                        // New connection: generate new ID
                        match ConnectionProfile::new(
                            name, host, port, database, user, password, ssl_mode,
                        ) {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = self
                                    .action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()));
                                return Ok(());
                            }
                        }
                    }
                };
                let id = profile.id.clone();
                let dsn = profile.to_dsn();
                let name = profile.name.as_str().to_string();
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                let provider = Arc::clone(&self.metadata_provider);
                let cache = self.metadata_cache.clone();
                tokio::spawn(async move {
                    match provider.fetch_metadata(&dsn).await {
                        Ok(metadata) => {
                            cache.set(dsn.clone(), metadata).await;
                            match store.save(&profile) {
                                Ok(()) => {
                                    let _ = tx
                                        .send(Action::ConnectionSaveCompleted { id, dsn, name })
                                        .await;
                                }
                                Err(e) => {
                                    let _ =
                                        tx.send(Action::ConnectionSaveFailed(e.to_string())).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Action::MetadataFailed(e.to_string())).await;
                        }
                    }
                });
                Ok(())
            }

            Effect::LoadConnectionForEdit { id } => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.find_by_id(&id) {
                    Ok(Some(profile)) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoaded(Box::new(profile)));
                    }
                    Ok(None) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoadFailed(
                            "Connection not found".to_string(),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Action::ConnectionEditLoadFailed(e.to_string()));
                    }
                });
                Ok(())
            }

            Effect::LoadConnections => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.load_all() {
                    Ok(profiles) => {
                        let _ = tx.blocking_send(Action::ConnectionsLoaded(profiles));
                    }
                    Err(_) => {
                        // On error, send empty list to avoid blocking UI
                        let _ = tx.blocking_send(Action::ConnectionsLoaded(vec![]));
                    }
                });
                Ok(())
            }

            Effect::DeleteConnection { id } => {
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || match store.delete(&id) {
                    Ok(()) => {
                        let _ = tx.blocking_send(Action::ConnectionDeleted(id));
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Action::ConnectionDeleteFailed(e.to_string()));
                    }
                });
                Ok(())
            }

            Effect::CacheInvalidate { dsn } => {
                self.metadata_cache.invalidate(&dsn).await;
                Ok(())
            }

            Effect::ClearCompletionEngineCache => {
                completion_engine.borrow_mut().clear_table_cache();
                Ok(())
            }

            Effect::FetchMetadata { dsn } => {
                if let Some(cached) = self.metadata_cache.get(&dsn).await {
                    let _ = self
                        .action_tx
                        .send(Action::MetadataLoaded(Box::new(cached)))
                        .await;
                } else {
                    let provider = Arc::clone(&self.metadata_provider);
                    let cache = self.metadata_cache.clone();
                    let tx = self.action_tx.clone();

                    tokio::spawn(async move {
                        match provider.fetch_metadata(&dsn).await {
                            Ok(metadata) => {
                                cache.set(dsn, metadata.clone()).await;
                                let _ = tx.send(Action::MetadataLoaded(Box::new(metadata))).await;
                            }
                            Err(e) => {
                                let _ = tx.send(Action::MetadataFailed(e.to_string())).await;
                            }
                        }
                    });
                }
                Ok(())
            }

            Effect::FetchTableDetail {
                dsn,
                schema,
                table,
                generation,
            } => {
                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailLoaded(Box::new(detail), generation))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::PrefetchTableDetail { dsn, schema, table } => {
                let qualified_name = format!("{}.{}", schema, table);

                let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);
                if already_cached {
                    let _ = self
                        .action_tx
                        .send(Action::TableDetailAlreadyCached { schema, table })
                        .await;
                    return Ok(());
                }

                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailCached {
                                    schema,
                                    table,
                                    detail: Box::new(detail),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailCacheFailed {
                                    schema,
                                    table,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::ProcessPrefetchQueue => {
                let _ = self.action_tx.send(Action::ProcessPrefetchQueue).await;
                Ok(())
            }

            Effect::ExecutePreview {
                dsn,
                schema,
                table,
                generation,
                limit,
            } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor.execute_preview(&dsn, &schema, &table, limit).await {
                        Ok(result) => {
                            let _ = tx
                                .send(Action::QueryCompleted(Arc::new(result), generation))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::QueryFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
                Ok(())
            }

            Effect::ExecuteAdhoc { dsn, query } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor.execute_adhoc(&dsn, &query).await {
                        Ok(result) => {
                            let _ = tx.send(Action::QueryCompleted(Arc::new(result), 0)).await;
                        }
                        Err(e) => {
                            let _ = tx.send(Action::QueryFailed(e.to_string(), 0)).await;
                        }
                    }
                });
                Ok(())
            }

            Effect::CacheTableInCompletionEngine {
                qualified_name,
                table,
            } => {
                completion_engine
                    .borrow_mut()
                    .cache_table_detail(qualified_name, *table);
                Ok(())
            }

            Effect::TriggerCompletion => {
                let cursor = state.sql_modal.cursor;

                let missing = {
                    let engine = completion_engine.borrow();
                    engine.missing_tables(&state.sql_modal.content, state.cache.metadata.as_ref())
                };

                let prefetch_actions: Vec<Action> = missing
                    .into_iter()
                    .filter_map(|qualified_name| {
                        qualified_name.split_once('.').map(|(schema, table)| {
                            Action::PrefetchTableDetail {
                                schema: schema.to_string(),
                                table: table.to_string(),
                            }
                        })
                    })
                    .collect();

                for action in prefetch_actions {
                    let _ = self.action_tx.try_send(action);
                }

                let (candidates, token_len, visible) = {
                    let engine = completion_engine.borrow();
                    let token_len = engine.current_token_len(&state.sql_modal.content, cursor);
                    let recent_cols = state.sql_modal.completion.recent_columns_vec();
                    let candidates = engine.get_candidates(
                        &state.sql_modal.content,
                        cursor,
                        state.cache.metadata.as_ref(),
                        state.cache.table_detail.as_ref(),
                        &recent_cols,
                    );
                    let visible =
                        !candidates.is_empty() && !state.sql_modal.content.trim().is_empty();
                    (candidates, token_len, visible)
                };

                let _ = self
                    .action_tx
                    .send(Action::CompletionUpdated {
                        candidates,
                        trigger_position: cursor.saturating_sub(token_len),
                        visible,
                    })
                    .await;
                Ok(())
            }

            Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name,
            } => {
                let tables: Vec<ErTableInfo> = {
                    let engine = completion_engine.borrow();
                    engine
                        .table_details_iter()
                        .map(|(k, v)| ErTableInfo::from_table(k, v))
                        .collect()
                };

                if tables.is_empty() {
                    let _ = self
                        .action_tx
                        .send(Action::ErDiagramFailed(
                            "No table data loaded yet".to_string(),
                        ))
                        .await;
                    return Ok(());
                }

                let cache_dir = self.config_writer.get_cache_dir(&project_name)?;
                let exporter = Arc::clone(&self.er_exporter);
                spawn_er_diagram_task(
                    exporter,
                    tables,
                    total_tables,
                    cache_dir,
                    self.action_tx.clone(),
                );
                Ok(())
            }

            Effect::WriteErFailureLog { failed_tables } => {
                if let Ok(cache_dir) = self
                    .config_writer
                    .get_cache_dir(&state.runtime.project_name)
                {
                    tokio::task::spawn_blocking(move || {
                        let _ = write_er_failure_log_blocking(failed_tables, cache_dir);
                    });
                }
                Ok(())
            }

            Effect::Sequence(_) => {
                // Handled in run()
                Ok(())
            }

            Effect::DispatchActions(actions) => {
                for action in actions {
                    let _ = self.action_tx.send(action).await;
                }
                Ok(())
            }

            Effect::CopyToClipboard { content } => {
                let tx = self.action_tx.clone();
                tokio::task::spawn_blocking(move || {
                    if let Ok(mut clipboard) = arboard::Clipboard::new()
                        && clipboard.set_text(&content).is_ok()
                    {
                        let _ = tx.blocking_send(Action::ConnectionErrorCopied);
                    }
                });
                Ok(())
            }
        }
    }
}
