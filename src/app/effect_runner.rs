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
use crate::app::effect_handlers::{self, EffectContext};
use crate::app::ports::{
    ConfigWriter, ConnectionStore, DsnBuilder, ErDiagramExporter, ErLogWriter, MetadataProvider,
    QueryExecutor, Renderer, ServiceFileReader,
};
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::domain::DatabaseMetadata;

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
        services: &AppServices,
    ) -> Result<()> {
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        self.run_single(seq_effect, tui, state, completion_engine, services)
                            .await?;
                    }
                }
                single_effect => {
                    self.run_single(single_effect, tui, state, completion_engine, services)
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
        services: &AppServices,
    ) -> Result<()> {
        self.run_normal(effect, tui, state, completion_engine, services)
            .await
    }

    fn effect_context(&self) -> EffectContext<'_> {
        EffectContext {
            metadata_provider: &self.metadata_provider,
            query_executor: &self.query_executor,
            dsn_builder: &self.dsn_builder,
            er_exporter: &self.er_exporter,
            config_writer: &self.config_writer,
            er_log_writer: &self.er_log_writer,
            connection_store: &self.connection_store,
            service_file_reader: &self.service_file_reader,
            metadata_cache: &self.metadata_cache,
            action_tx: &self.action_tx,
        }
    }

    async fn run_normal<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        _services: &AppServices,
    ) -> Result<()> {
        let ctx = self.effect_context();

        match effect {
            Effect::Render => {
                let output = tui.draw(state, _services)?;
                if !state.ui.focus_mode {
                    state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
                }
                state.ui.result_viewport_plan = output.result_viewport_plan;
                state.ui.explorer_pane_height = output.explorer_pane_height;
                state.ui.inspector_pane_height = output.inspector_pane_height;
                state.ui.result_pane_height = output.result_pane_height;
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
            Effect::OpenFolder { path } => {
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&path).spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(&path).spawn();
                Ok(())
            }

            e @ (Effect::SaveAndConnect { .. }
            | Effect::LoadConnectionForEdit { .. }
            | Effect::LoadConnections
            | Effect::DeleteConnection { .. }
            | Effect::SwitchConnection { .. }
            | Effect::SwitchToService { .. }) => {
                effect_handlers::connection::run(e, &ctx, state).await
            }

            e @ (Effect::FetchMetadata { .. }
            | Effect::FetchTableDetail { .. }
            | Effect::PrefetchTableDetail { .. }
            | Effect::ProcessPrefetchQueue
            | Effect::DelayedProcessPrefetchQueue { .. }
            | Effect::CacheInvalidate { .. }) => {
                effect_handlers::metadata::run(e, &ctx, state, completion_engine).await
            }

            e @ (Effect::ExecutePreview { .. }
            | Effect::ExecuteAdhoc { .. }
            | Effect::ExecuteWrite { .. }
            | Effect::CountRowsForExport { .. }
            | Effect::ExportCsv { .. }) => effect_handlers::query::run(e, &ctx, state).await,

            e @ (Effect::GenerateErDiagramFromCache { .. }
            | Effect::ExtractFkNeighbors { .. }
            | Effect::WriteErFailureLog { .. }
            | Effect::SmartErRefresh { .. }) => {
                effect_handlers::er::run(e, &ctx, state, completion_engine).await
            }

            e @ (Effect::CacheTableInCompletionEngine { .. }
            | Effect::EvictTablesFromCompletionCache { .. }
            | Effect::ClearCompletionEngineCache
            | Effect::ResizeCompletionCache { .. }
            | Effect::TriggerCompletion) => {
                effect_handlers::completion::run(e, &ctx, state, completion_engine).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect_handlers::test_support::*;
    use crate::app::ports::RenderOutput;
    use crate::app::ports::connection_store::MockConnectionStore;
    use crate::app::ports::metadata::MockMetadataProvider;
    use crate::app::ports::query_executor::MockQueryExecutor;
    use crate::app::services::AppServices;
    use color_eyre::eyre::Result;
    use tokio::sync::mpsc;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(&mut self, _state: &mut AppState, _services: &AppServices) -> Result<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    mod render {
        use super::*;

        #[tokio::test]
        async fn render_calls_draw() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::Render],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();
        }
    }

    mod dispatch_actions {
        use super::*;

        #[tokio::test]
        async fn dispatches_all_actions() {
            let (tx, mut rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = NoopRenderer;

            runner
                .run(
                    vec![Effect::DispatchActions(vec![
                        Action::ProcessPrefetchQueue,
                        Action::ProcessPrefetchQueue,
                    ])],
                    &mut renderer,
                    state,
                    &ce,
                    &AppServices::stub(),
                )
                .await
                .unwrap();

            let a1 = rx.recv().await.unwrap();
            let a2 = rx.recv().await.unwrap();
            assert!(matches!(a1, Action::ProcessPrefetchQueue));
            assert!(matches!(a2, Action::ProcessPrefetchQueue));
        }
    }
}
