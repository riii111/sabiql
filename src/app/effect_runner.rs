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

use crate::app::action::{
    Action, ConnectionTarget, ConnectionsLoadedPayload, SmartErRefreshError, SmartErRefreshResult,
};
use crate::app::cache::TtlCache;
use crate::app::completion::CompletionEngine;
use crate::app::effect::Effect;
use crate::app::er_task::spawn_er_diagram_task;
use crate::app::ports::{
    ConfigWriter, ConnectionStore, DsnBuilder, ErDiagramExporter, ErLogWriter, MetadataProvider,
    QueryExecutor, Renderer, ServiceFileError, ServiceFileReader,
};
use crate::app::state::AppState;
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, ErTableInfo};

pub struct EffectRunner {
    metadata_provider: Arc<dyn MetadataProvider>,
    query_executor: Arc<dyn QueryExecutor>,
    dsn_builder: Arc<dyn DsnBuilder>,
    er_exporter: Arc<dyn ErDiagramExporter>,
    config_writer: Arc<dyn ConfigWriter>,
    er_log_writer: Arc<dyn ErLogWriter>,
    connection_store: Arc<dyn ConnectionStore>,
    service_file_reader: Arc<dyn ServiceFileReader>,
    metadata_cache: TtlCache<String, DatabaseMetadata>,
    action_tx: mpsc::Sender<Action>,
}

pub struct EffectRunnerBuilder {
    metadata_provider: Option<Arc<dyn MetadataProvider>>,
    query_executor: Option<Arc<dyn QueryExecutor>>,
    dsn_builder: Option<Arc<dyn DsnBuilder>>,
    er_exporter: Option<Arc<dyn ErDiagramExporter>>,
    config_writer: Option<Arc<dyn ConfigWriter>>,
    er_log_writer: Option<Arc<dyn ErLogWriter>>,
    connection_store: Option<Arc<dyn ConnectionStore>>,
    service_file_reader: Option<Arc<dyn ServiceFileReader>>,
    metadata_cache: Option<TtlCache<String, DatabaseMetadata>>,
    action_tx: Option<mpsc::Sender<Action>>,
}

impl EffectRunnerBuilder {
    pub fn metadata_provider(mut self, v: Arc<dyn MetadataProvider>) -> Self {
        self.metadata_provider = Some(v);
        self
    }
    pub fn query_executor(mut self, v: Arc<dyn QueryExecutor>) -> Self {
        self.query_executor = Some(v);
        self
    }
    pub fn dsn_builder(mut self, v: Arc<dyn DsnBuilder>) -> Self {
        self.dsn_builder = Some(v);
        self
    }
    pub fn er_exporter(mut self, v: Arc<dyn ErDiagramExporter>) -> Self {
        self.er_exporter = Some(v);
        self
    }
    pub fn config_writer(mut self, v: Arc<dyn ConfigWriter>) -> Self {
        self.config_writer = Some(v);
        self
    }
    pub fn er_log_writer(mut self, v: Arc<dyn ErLogWriter>) -> Self {
        self.er_log_writer = Some(v);
        self
    }
    pub fn connection_store(mut self, v: Arc<dyn ConnectionStore>) -> Self {
        self.connection_store = Some(v);
        self
    }
    pub fn service_file_reader(mut self, v: Arc<dyn ServiceFileReader>) -> Self {
        self.service_file_reader = Some(v);
        self
    }
    pub fn metadata_cache(mut self, v: TtlCache<String, DatabaseMetadata>) -> Self {
        self.metadata_cache = Some(v);
        self
    }
    pub fn action_tx(mut self, v: mpsc::Sender<Action>) -> Self {
        self.action_tx = Some(v);
        self
    }

    pub fn build(self) -> EffectRunner {
        EffectRunner {
            metadata_provider: self
                .metadata_provider
                .expect("metadata_provider is required"),
            query_executor: self.query_executor.expect("query_executor is required"),
            dsn_builder: self.dsn_builder.expect("dsn_builder is required"),
            er_exporter: self.er_exporter.expect("er_exporter is required"),
            config_writer: self.config_writer.expect("config_writer is required"),
            er_log_writer: self.er_log_writer.expect("er_log_writer is required"),
            connection_store: self.connection_store.expect("connection_store is required"),
            service_file_reader: self
                .service_file_reader
                .expect("service_file_reader is required"),
            metadata_cache: self.metadata_cache.expect("metadata_cache is required"),
            action_tx: self.action_tx.expect("action_tx is required"),
        }
    }
}

impl EffectRunner {
    pub fn builder() -> EffectRunnerBuilder {
        EffectRunnerBuilder {
            metadata_provider: None,
            query_executor: None,
            dsn_builder: None,
            er_exporter: None,
            config_writer: None,
            er_log_writer: None,
            connection_store: None,
            service_file_reader: None,
            metadata_cache: None,
            action_tx: None,
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
                state.ui.explorer_pane_height = output.explorer_pane_height;
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
                                self.action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()))
                                    .ok();
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
                                self.action_tx
                                    .blocking_send(Action::ConnectionSaveFailed(e.to_string()))
                                    .ok();
                                return Ok(());
                            }
                        }
                    }
                };
                let id = profile.id.clone();
                let dsn = self.dsn_builder.build_dsn(&profile);
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
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

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
                let store = Arc::clone(&self.connection_store);
                let reader = Arc::clone(&self.service_file_reader);
                let tx = self.action_tx.clone();

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
                let store = Arc::clone(&self.connection_store);
                let tx = self.action_tx.clone();

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

            Effect::CacheInvalidate { dsn } => {
                self.metadata_cache.invalidate(&dsn).await;
                Ok(())
            }

            Effect::ClearCompletionEngineCache => {
                completion_engine.borrow_mut().clear_table_cache();
                Ok(())
            }

            Effect::ResizeCompletionCache { capacity } => {
                completion_engine.borrow_mut().resize_cache(capacity);
                Ok(())
            }

            Effect::FetchMetadata { dsn } => {
                if let Some(cached) = self.metadata_cache.get(&dsn).await {
                    self.action_tx
                        .send(Action::MetadataLoaded(Box::new(cached)))
                        .await
                        .ok();
                } else {
                    let provider = Arc::clone(&self.metadata_provider);
                    let cache = self.metadata_cache.clone();
                    let tx = self.action_tx.clone();

                    tokio::spawn(async move {
                        match provider.fetch_metadata(&dsn).await {
                            Ok(metadata) => {
                                cache.set(dsn, metadata.clone()).await;
                                tx.send(Action::MetadataLoaded(Box::new(metadata)))
                                    .await
                                    .ok();
                            }
                            Err(e) => {
                                tx.send(Action::MetadataFailed(e.to_string())).await.ok();
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
                            tx.send(Action::TableDetailLoaded(Box::new(detail), generation))
                                .await
                                .ok();
                        }
                        Err(e) => {
                            tx.send(Action::TableDetailFailed(e.to_string(), generation))
                                .await
                                .ok();
                        }
                    }
                });
                Ok(())
            }

            Effect::PrefetchTableDetail { dsn, schema, table } => {
                let qualified_name = format!("{}.{}", schema, table);

                let already_cached = completion_engine.borrow().has_cached_table(&qualified_name);
                if already_cached {
                    self.action_tx
                        .send(Action::TableDetailAlreadyCached { schema, table })
                        .await
                        .ok();
                    return Ok(());
                }

                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    let result = tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        provider.fetch_table_detail(&dsn, &schema, &table),
                    )
                    .await;
                    match result {
                        Ok(Ok(detail)) => {
                            tx.send(Action::TableDetailCached {
                                schema,
                                table,
                                detail: Box::new(detail),
                            })
                            .await
                            .ok();
                        }
                        Ok(Err(e)) => {
                            tx.send(Action::TableDetailCacheFailed {
                                schema,
                                table,
                                error: e.to_string(),
                            })
                            .await
                            .ok();
                        }
                        Err(_) => {
                            tx.send(Action::TableDetailCacheFailed {
                                schema,
                                table,
                                error: "prefetch timeout".to_string(),
                            })
                            .await
                            .ok();
                        }
                    }
                });
                Ok(())
            }

            Effect::ProcessPrefetchQueue => {
                self.action_tx.send(Action::ProcessPrefetchQueue).await.ok();
                Ok(())
            }

            Effect::DelayedProcessPrefetchQueue { delay_secs } => {
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                    tx.send(Action::ProcessPrefetchQueue).await.ok();
                });
                Ok(())
            }

            Effect::ExecutePreview {
                dsn,
                schema,
                table,
                generation,
                limit,
                offset,
                target_page,
            } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor
                        .execute_preview(&dsn, &schema, &table, limit, offset)
                        .await
                    {
                        Ok(result) => {
                            tx.send(Action::QueryCompleted {
                                result: Arc::new(result),
                                generation,
                                target_page: Some(target_page),
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            tx.send(Action::QueryFailed(e.to_string(), generation))
                                .await
                                .ok();
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
                            tx.send(Action::QueryCompleted {
                                result: Arc::new(result),
                                generation: 0,
                                target_page: None,
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            tx.send(Action::QueryFailed(e.to_string(), 0)).await.ok();
                        }
                    }
                });
                Ok(())
            }

            Effect::ExecuteWrite { dsn, query } => {
                let executor = Arc::clone(&self.query_executor);
                let tx = self.action_tx.clone();

                tokio::spawn(async move {
                    match executor.execute_write(&dsn, &query).await {
                        Ok(result) => {
                            tx.send(Action::ExecuteWriteSucceeded {
                                affected_rows: result.affected_rows,
                            })
                            .await
                            .ok();
                        }
                        Err(e) => {
                            tx.send(Action::ExecuteWriteFailed(e.to_string()))
                                .await
                                .ok();
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
                    self.action_tx.try_send(action).ok();
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

                self.action_tx
                    .send(Action::CompletionUpdated {
                        candidates,
                        trigger_position: cursor.saturating_sub(token_len),
                        visible,
                    })
                    .await
                    .ok();
                Ok(())
            }

            Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name,
                target_tables,
            } => {
                use crate::domain::er::{er_output_filename, fk_reachable_tables_multi};

                let all_tables: Vec<ErTableInfo> = {
                    let engine = completion_engine.borrow();
                    engine
                        .table_details_iter()
                        .map(|(k, v)| ErTableInfo::from_table(k, v))
                        .collect()
                };

                if all_tables.is_empty() {
                    self.action_tx
                        .send(Action::ErDiagramFailed(
                            "No table data loaded yet".to_string(),
                        ))
                        .await
                        .ok();
                    return Ok(());
                }

                let total = all_tables.len();
                let filename = er_output_filename(&target_tables, total);
                let tables = if target_tables.is_empty() || target_tables.len() == total {
                    all_tables
                } else {
                    fk_reachable_tables_multi(&all_tables, &target_tables, 1)
                };

                if tables.is_empty() {
                    self.action_tx
                        .send(Action::ErDiagramFailed(
                            "Selected tables not found in cached data".to_string(),
                        ))
                        .await
                        .ok();
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
                    filename,
                );
                Ok(())
            }

            Effect::ExtractFkNeighbors { seed_tables } => {
                use crate::domain::er::fk_neighbors_of_seeds;
                use std::collections::HashSet;

                let seed_set: HashSet<&str> = seed_tables.iter().map(|s| s.as_str()).collect();

                let (cached_seeds, cached_names): (Vec<ErTableInfo>, HashSet<String>) = {
                    let engine = completion_engine.borrow();
                    let seeds: Vec<ErTableInfo> = engine
                        .table_details_iter()
                        .filter(|(k, _)| seed_set.contains(k.as_str()))
                        .map(|(k, v)| ErTableInfo::from_table(k, v))
                        .collect();
                    let all_cached: HashSet<String> = engine
                        .table_details_iter()
                        .map(|(k, _)| k.clone())
                        .collect();
                    (seeds, all_cached)
                };

                let neighbors = fk_neighbors_of_seeds(&cached_seeds, &seed_set, &cached_names);

                self.action_tx
                    .send(Action::FkNeighborsDiscovered { tables: neighbors })
                    .await
                    .ok();
                Ok(())
            }

            Effect::WriteErFailureLog { failed_tables } => {
                if let Ok(cache_dir) = self
                    .config_writer
                    .get_cache_dir(&state.runtime.project_name)
                {
                    let writer = Arc::clone(&self.er_log_writer);
                    tokio::task::spawn_blocking(move || {
                        // Log write failure is intentionally ignored: the app has no
                        // output channel (TUI is active) and tracing is not available.
                        writer.write_er_failure_log(failed_tables, cache_dir).ok();
                    });
                }
                Ok(())
            }

            Effect::SmartErRefresh { dsn, run_id } => {
                let provider = Arc::clone(&self.metadata_provider);
                let tx = self.action_tx.clone();

                let old_signatures = state.er_preparation.last_signatures.clone();
                let cached_tables: std::collections::HashSet<String> = {
                    let engine = completion_engine.borrow();
                    engine
                        .table_details_iter()
                        .map(|(k, _)| k.clone())
                        .collect()
                };

                tokio::spawn(async move {
                    // TTL bypass: call provider directly instead of metadata_cache
                    let new_metadata = match provider.fetch_metadata(&dsn).await {
                        Ok(m) => m,
                        Err(e) => {
                            tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                                run_id,
                                error: e.to_string(),
                                new_metadata: None,
                            }))
                            .await
                            .ok();
                            return;
                        }
                    };

                    let new_sigs_vec = match provider.fetch_table_signatures(&dsn).await {
                        Ok(s) => s,
                        Err(e) => {
                            tx.send(Action::SmartErRefreshFailed(SmartErRefreshError {
                                run_id,
                                error: e.to_string(),
                                new_metadata: Some(Box::new(new_metadata)),
                            }))
                            .await
                            .ok();
                            return;
                        }
                    };

                    let new_signatures: std::collections::HashMap<String, String> = new_sigs_vec
                        .iter()
                        .map(|s| (s.qualified_name(), s.signature.clone()))
                        .collect();

                    let old_names: std::collections::HashSet<&str> =
                        old_signatures.keys().map(|s| s.as_str()).collect();
                    let new_names: std::collections::HashSet<&str> =
                        new_signatures.keys().map(|s| s.as_str()).collect();

                    let added_tables: Vec<String> = new_names
                        .difference(&old_names)
                        .map(|s| s.to_string())
                        .collect();
                    let removed_tables: Vec<String> = old_names
                        .difference(&new_names)
                        .map(|s| s.to_string())
                        .collect();

                    let stale_tables: Vec<String> = new_signatures
                        .iter()
                        .filter(|(name, sig)| {
                            old_signatures
                                .get(name.as_str())
                                .is_some_and(|old_sig| old_sig != *sig)
                        })
                        .map(|(name, _)| name.clone())
                        .collect();

                    let missing_in_cache: Vec<String> = new_names
                        .iter()
                        .filter(|name| !cached_tables.contains(**name))
                        .map(|s| s.to_string())
                        .collect();

                    tx.send(Action::SmartErRefreshCompleted(SmartErRefreshResult {
                        run_id,
                        new_metadata: Box::new(new_metadata),
                        stale_tables,
                        added_tables,
                        removed_tables,
                        missing_in_cache,
                        new_signatures,
                    }))
                    .await
                    .ok();
                });
                Ok(())
            }

            Effect::EvictTablesFromCompletionCache { tables } => {
                completion_engine.borrow_mut().evict_tables(&tables);
                Ok(())
            }

            Effect::Sequence(_) => {
                // Handled in run()
                Ok(())
            }

            Effect::DispatchActions(actions) => {
                for action in actions {
                    self.action_tx.send(action).await.ok();
                }
                Ok(())
            }

            Effect::SwitchConnection { connection_index } => {
                if let Some(profile) = state.connections.get(connection_index) {
                    let dsn = self.dsn_builder.build_dsn(profile);
                    let name = profile.display_name().to_string();
                    let id = profile.id.clone();
                    self.action_tx
                        .send(Action::SwitchConnection(ConnectionTarget { id, dsn, name }))
                        .await
                        .ok();
                }
                Ok(())
            }

            Effect::SwitchToService { service_index } => {
                if let Some(entry) = state.service_entries.get(service_index) {
                    let id = entry.connection_id();
                    let dsn = entry.to_dsn();
                    let name = entry.display_name().to_owned();
                    self.action_tx
                        .send(Action::SwitchConnection(ConnectionTarget { id, dsn, name }))
                        .await
                        .ok();
                }
                Ok(())
            }

            Effect::CopyToClipboard {
                content,
                on_success,
                on_failure,
            } => {
                let tx = self.action_tx.clone();
                tokio::task::spawn_blocking(move || {
                    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&content)) {
                        Ok(()) => {
                            if let Some(action) = on_success {
                                tx.blocking_send(action).ok();
                            }
                        }
                        Err(e) => {
                            if let Some(action) = on_failure {
                                tx.blocking_send(action).ok();
                            } else {
                                tx.blocking_send(Action::CopyFailed(e.to_string())).ok();
                            }
                        }
                    }
                });
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::completion::CompletionEngine;
    use crate::app::ports::connection_store::MockConnectionStore;
    use crate::app::ports::metadata::MockMetadataProvider;
    use crate::app::ports::query_executor::MockQueryExecutor;
    use crate::app::ports::{
        ConfigWriter, ErDiagramExporter, ErExportResult, ErLogWriter, RenderOutput, Renderer,
    };
    use crate::domain::connection::ConnectionId;
    use crate::domain::{DatabaseMetadata, QueryResult, QuerySource};
    use color_eyre::eyre::Result;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use tokio::sync::mpsc;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(&mut self, _state: &mut AppState) -> Result<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    struct NoopConfigWriter;
    impl ConfigWriter for NoopConfigWriter {
        fn get_cache_dir(&self, _project_name: &str) -> Result<PathBuf> {
            Ok(PathBuf::from("/tmp"))
        }
    }

    struct NoopErExporter;
    impl ErDiagramExporter for NoopErExporter {
        fn generate_and_export(
            &self,
            _tables: &[crate::domain::ErTableInfo],
            _filename: &str,
            _cache_dir: &Path,
        ) -> ErExportResult<PathBuf> {
            Ok(PathBuf::from("/tmp/er.svg"))
        }
    }

    struct NoopErLogWriter;
    impl ErLogWriter for NoopErLogWriter {
        fn write_er_failure_log(
            &self,
            _failed_tables: Vec<(String, String)>,
            _cache_dir: PathBuf,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct NoopDsnBuilder;
    impl DsnBuilder for NoopDsnBuilder {
        fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
            String::new()
        }
    }

    struct NoopServiceFileReader;
    impl ServiceFileReader for NoopServiceFileReader {
        fn read_services(
            &self,
        ) -> Result<
            (
                Vec<crate::domain::connection::ServiceEntry>,
                std::path::PathBuf,
            ),
            crate::app::ports::ServiceFileError,
        > {
            Ok((vec![], std::path::PathBuf::new()))
        }
    }

    fn make_runner(
        metadata_provider: Arc<dyn MetadataProvider>,
        query_executor: Arc<dyn QueryExecutor>,
        connection_store: Arc<dyn ConnectionStore>,
        cache: TtlCache<String, DatabaseMetadata>,
        action_tx: mpsc::Sender<Action>,
    ) -> EffectRunner {
        EffectRunner::builder()
            .metadata_provider(metadata_provider)
            .query_executor(query_executor)
            .dsn_builder(Arc::new(NoopDsnBuilder))
            .er_exporter(Arc::new(NoopErExporter))
            .config_writer(Arc::new(NoopConfigWriter))
            .er_log_writer(Arc::new(NoopErLogWriter))
            .connection_store(connection_store)
            .service_file_reader(Arc::new(NoopServiceFileReader))
            .metadata_cache(cache)
            .action_tx(action_tx)
            .build()
    }

    fn sample_metadata() -> DatabaseMetadata {
        DatabaseMetadata {
            database_name: "testdb".to_string(),
            schemas: vec![],
            tables: vec![],
            fetched_at: Instant::now(),
        }
    }

    fn sample_query_result() -> QueryResult {
        QueryResult {
            query: "SELECT 1".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            row_count: 1,
            execution_time_ms: 5,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
        }
    }

    mod fetch_metadata {
        use super::*;

        #[tokio::test]
        async fn cache_hit_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().never();

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            cache.set("dsn://test".to_string(), sample_metadata()).await;

            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://test".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::MetadataLoaded(_)),
                "expected MetadataLoaded, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn cache_miss_returns_metadata_loaded() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider
                .expect_fetch_metadata()
                .once()
                .returning(|_| Ok(sample_metadata()));

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://miss".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::MetadataLoaded(_)),
                "expected MetadataLoaded, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn provider_error_returns_metadata_failed() {
            let mut mock_provider = MockMetadataProvider::new();
            mock_provider.expect_fetch_metadata().once().returning(|_| {
                Err(crate::app::ports::MetadataError::ConnectionFailed(
                    "timeout".to_string(),
                ))
            });

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(mock_provider),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::FetchMetadata {
                        dsn: "dsn://err".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::MetadataFailed(_)),
                "expected MetadataFailed, got {:?}",
                action
            );
        }
    }

    mod execute_preview {
        use super::*;

        #[tokio::test]
        async fn success_returns_query_completed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| Ok(sample_query_result()));

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExecutePreview {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::QueryCompleted { .. }),
                "expected QueryCompleted, got {:?}",
                action
            );
        }

        #[tokio::test]
        async fn error_returns_query_failed() {
            let mut mock_executor = MockQueryExecutor::new();
            mock_executor
                .expect_execute_preview()
                .once()
                .returning(|_, _, _, _, _| {
                    Err(crate::app::ports::MetadataError::QueryFailed(
                        "syntax error".to_string(),
                    ))
                });

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(mock_executor),
                Arc::new(MockConnectionStore::new()),
                cache,
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::ExecutePreview {
                        dsn: "dsn://test".to_string(),
                        schema: "public".to_string(),
                        table: "users".to_string(),
                        generation: 1,
                        limit: 100,
                        offset: 0,
                        target_page: 0,
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(
                matches!(action, Action::QueryFailed(_, _)),
                "expected QueryFailed, got {:?}",
                action
            );
        }
    }

    mod delete_connection {
        use super::*;

        #[tokio::test]
        async fn success_returns_connection_deleted() {
            let mut mock_store = MockConnectionStore::new();
            mock_store.expect_delete().once().returning(|_| Ok(()));

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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

            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
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
                .run(vec![Effect::LoadConnections], &mut renderer, state, &ce)
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

    mod cache_invalidate {
        use super::*;

        #[tokio::test]
        async fn invalidate_removes_cache_entry() {
            let cache: TtlCache<String, DatabaseMetadata> = TtlCache::new(300);
            cache
                .set("dsn://target".to_string(), sample_metadata())
                .await;

            assert!(cache.get(&"dsn://target".to_string()).await.is_some());

            let (tx, _rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                cache.clone(),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::CacheInvalidate {
                        dsn: "dsn://target".to_string(),
                    }],
                    &mut renderer,
                    state,
                    &ce,
                )
                .await
                .unwrap();

            assert!(cache.get(&"dsn://target".to_string()).await.is_none());
        }
    }

    mod switch_connection {
        use super::*;
        use crate::domain::connection::SslMode;

        struct FakeDsnBuilder;
        impl DsnBuilder for FakeDsnBuilder {
            fn build_dsn(&self, profile: &ConnectionProfile) -> String {
                format!(
                    "fake://{}:{}/{}",
                    profile.host, profile.port, profile.database
                )
            }
        }

        fn make_runner_with_dsn_builder(action_tx: mpsc::Sender<Action>) -> EffectRunner {
            EffectRunner::builder()
                .metadata_provider(Arc::new(MockMetadataProvider::new()))
                .query_executor(Arc::new(MockQueryExecutor::new()))
                .dsn_builder(Arc::new(FakeDsnBuilder))
                .er_exporter(Arc::new(NoopErExporter))
                .config_writer(Arc::new(NoopConfigWriter))
                .er_log_writer(Arc::new(NoopErLogWriter))
                .connection_store(Arc::new(MockConnectionStore::new()))
                .service_file_reader(Arc::new(NoopServiceFileReader))
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
            state.connections = vec![profile];

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
                )
                .await
                .unwrap();

            let action = rx.recv().await.expect("action dispatched");
            match action {
                Action::SwitchConnection(ConnectionTarget { id, dsn, name }) => {
                    assert_eq!(dsn, "fake://db.example.com:5432/mydb");
                    assert_eq!(name, "My DB");
                    assert_eq!(id, state.connections[0].id);
                }
                other => panic!("expected SwitchConnection, got {:?}", other),
            }
        }

        #[tokio::test]
        async fn out_of_bounds_index_is_noop() {
            let (tx, mut rx) = mpsc::channel::<Action>(16);
            let runner = make_runner_with_dsn_builder(tx);

            let mut state = AppState::new("test".to_string());
            // no connections

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
                )
                .await
                .unwrap();

            assert!(rx.try_recv().is_err());
        }
    }
}
