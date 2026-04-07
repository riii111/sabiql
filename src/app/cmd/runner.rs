// RefCell Borrow Safety: when effects need data from `completion_engine`,
// the borrow MUST be dropped before any await point.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::browse as cmd_browse;
use crate::app::cmd::cache::TtlCache;
use crate::app::cmd::completion_engine::CompletionEngine;
use crate::app::cmd::connection as cmd_connection;
use crate::app::cmd::effect::Effect;
use crate::app::cmd::er::handler as cmd_er;
use crate::app::cmd::sql_editor::completion as cmd_completion;
use crate::app::cmd::sql_editor::query_history as cmd_query_history;
use crate::app::cmd::utility as cmd_utility;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::ui_state::scroll_max_offset;
use crate::app::ports::{
    ClipboardWriter, ConfigWriter, ConnectionStore, DsnBuilder, ErDiagramExporter, ErLogWriter,
    FolderOpener, MetadataProvider, QueryExecutor, QueryHistoryStore, Renderer, ServiceFileReader,
};
use crate::app::services::AppServices;
use crate::app::update::action::Action;
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
    clipboard: Arc<dyn ClipboardWriter>,
    folder_opener: Arc<dyn FolderOpener>,
    query_history_store: Arc<dyn QueryHistoryStore>,
    metadata_cache: TtlCache<String, Arc<DatabaseMetadata>>,
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
    clipboard: Option<Arc<dyn ClipboardWriter>>,
    folder_opener: Option<Arc<dyn FolderOpener>>,
    query_history_store: Option<Arc<dyn QueryHistoryStore>>,
    metadata_cache: Option<TtlCache<String, Arc<DatabaseMetadata>>>,
    action_tx: Option<mpsc::Sender<Action>>,
}

impl EffectRunnerBuilder {
    #[must_use]
    pub fn metadata_provider(mut self, v: Arc<dyn MetadataProvider>) -> Self {
        self.metadata_provider = Some(v);
        self
    }
    #[must_use]
    pub fn query_executor(mut self, v: Arc<dyn QueryExecutor>) -> Self {
        self.query_executor = Some(v);
        self
    }
    #[must_use]
    pub fn dsn_builder(mut self, v: Arc<dyn DsnBuilder>) -> Self {
        self.dsn_builder = Some(v);
        self
    }
    #[must_use]
    pub fn er_exporter(mut self, v: Arc<dyn ErDiagramExporter>) -> Self {
        self.er_exporter = Some(v);
        self
    }
    #[must_use]
    pub fn config_writer(mut self, v: Arc<dyn ConfigWriter>) -> Self {
        self.config_writer = Some(v);
        self
    }
    #[must_use]
    pub fn er_log_writer(mut self, v: Arc<dyn ErLogWriter>) -> Self {
        self.er_log_writer = Some(v);
        self
    }
    #[must_use]
    pub fn connection_store(mut self, v: Arc<dyn ConnectionStore>) -> Self {
        self.connection_store = Some(v);
        self
    }
    #[must_use]
    pub fn service_file_reader(mut self, v: Arc<dyn ServiceFileReader>) -> Self {
        self.service_file_reader = Some(v);
        self
    }
    #[must_use]
    pub fn clipboard(mut self, v: Arc<dyn ClipboardWriter>) -> Self {
        self.clipboard = Some(v);
        self
    }
    #[must_use]
    pub fn folder_opener(mut self, v: Arc<dyn FolderOpener>) -> Self {
        self.folder_opener = Some(v);
        self
    }
    #[must_use]
    pub fn query_history_store(mut self, v: Arc<dyn QueryHistoryStore>) -> Self {
        self.query_history_store = Some(v);
        self
    }
    #[must_use]
    pub fn metadata_cache(mut self, v: TtlCache<String, Arc<DatabaseMetadata>>) -> Self {
        self.metadata_cache = Some(v);
        self
    }
    #[must_use]
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
            clipboard: self.clipboard.expect("clipboard is required"),
            folder_opener: self.folder_opener.expect("folder_opener is required"),
            query_history_store: self
                .query_history_store
                .expect("query_history_store is required"),
            metadata_cache: self.metadata_cache.expect("metadata_cache is required"),
            action_tx: self.action_tx.expect("action_tx is required"),
        }
    }
}

impl EffectRunner {
    pub fn action_tx(&self) -> &mpsc::Sender<Action> {
        &self.action_tx
    }

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
            clipboard: None,
            folder_opener: None,
            query_history_store: None,
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
    ) -> Result<Vec<Action>> {
        let mut dispatched = Vec::new();
        for effect in effects {
            match effect {
                Effect::Sequence(seq_effects) => {
                    for seq_effect in seq_effects {
                        dispatched.extend(
                            self.run_single(seq_effect, tui, state, completion_engine, services)
                                .await?,
                        );
                    }
                }
                single_effect => {
                    dispatched.extend(
                        self.run_single(single_effect, tui, state, completion_engine, services)
                            .await?,
                    );
                }
            }
        }
        Ok(dispatched)
    }

    async fn run_single<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        services: &AppServices,
    ) -> Result<Vec<Action>> {
        self.run_normal(effect, tui, state, completion_engine, services)
            .await
    }

    async fn run_normal<T: Renderer>(
        &self,
        effect: Effect,
        tui: &mut T,
        state: &mut AppState,
        completion_engine: &RefCell<CompletionEngine>,
        services: &AppServices,
    ) -> Result<Vec<Action>> {
        match effect {
            Effect::Render => {
                let now = Instant::now();
                let output = tui.draw(state, services, now)?;
                if !state.ui.is_focus_mode() {
                    state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
                }
                state.ui.result_viewport_plan = output.result_viewport_plan;
                state.ui.result_widths_cache = output.result_widths_cache;
                state.ui.explorer_pane_height = output.explorer_pane_height;
                state.ui.explorer_content_width = output.explorer_content_width;
                let max_name_width = state
                    .tables()
                    .iter()
                    .map(|table| table.qualified_name().chars().count())
                    .max()
                    .unwrap_or(0);
                let max_offset = scroll_max_offset(max_name_width, state.ui.explorer_content_width);
                state.ui.explorer_horizontal_offset =
                    state.ui.explorer_horizontal_offset.min(max_offset);
                state.ui.inspector_pane_height = output.inspector_pane_height;
                state.ui.result_pane_height = output.result_pane_height;
                if let Some(width) = output.command_line_visible_width {
                    state.command_line_visible_width = width;
                }
                if let Some(height) = output.connection_list_pane_height {
                    state.ui.connection_list_pane_height = height;
                }
                if let Some(height) = output.table_picker_pane_height {
                    state.ui.table_picker.pane_height = height;
                }
                if let Some(width) = output.table_picker_filter_visible_width {
                    state.ui.table_picker.filter_visible_width = width;
                }
                if let Some(height) = output.er_picker_pane_height {
                    state.ui.er_picker.pane_height = height;
                }
                if let Some(width) = output.er_picker_filter_visible_width {
                    state.ui.er_picker.filter_visible_width = width;
                }
                if let Some(height) = output.query_history_picker_pane_height {
                    state.query_history_picker.pane_height = height;
                }
                if let Some(visible_rows) = output.jsonb_detail_editor_visible_rows {
                    state.ui.jsonb_detail_editor_visible_rows = visible_rows;
                    state.jsonb_detail.editor_mut().update_scroll(visible_rows);
                }
                state.confirm_dialog.preview_viewport_height =
                    output.confirm_preview_viewport_height;
                state.confirm_dialog.preview_content_height = output.confirm_preview_content_height;
                state.confirm_dialog.preview_scroll = output.confirm_preview_scroll;
                if let Some(height) = output.explain_compare_viewport_height {
                    state.explain.compare_viewport_height = Some(height);
                }
                Ok(vec![])
            }

            Effect::Sequence(_) => {
                // Handled in run()
                Ok(vec![])
            }
            Effect::DispatchActions(actions) => Ok(actions),

            e @ (Effect::CopyToClipboard { .. } | Effect::OpenFolder { .. }) => {
                cmd_utility::run(e, &self.action_tx, &self.clipboard, &self.folder_opener).await?;
                Ok(vec![])
            }

            e @ (Effect::SaveAndConnect { .. }
            | Effect::LoadConnectionForEdit { .. }
            | Effect::LoadConnections
            | Effect::DeleteConnection { .. }
            | Effect::SwitchConnection { .. }
            | Effect::SwitchToService { .. }) => {
                cmd_connection::run(
                    e,
                    &self.action_tx,
                    &self.dsn_builder,
                    &self.metadata_provider,
                    &self.metadata_cache,
                    &self.connection_store,
                    &self.service_file_reader,
                    state,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::FetchMetadata { .. }
            | Effect::FetchTableDetail { .. }
            | Effect::PrefetchTableDetail { .. }
            | Effect::ProcessPrefetchQueue
            | Effect::DelayedProcessPrefetchQueue { .. }
            | Effect::CacheInvalidate { .. }) => {
                cmd_browse::metadata::run(
                    e,
                    &self.action_tx,
                    &self.metadata_provider,
                    &self.metadata_cache,
                    state,
                    completion_engine,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::ExecutePreview { .. }
            | Effect::ExecuteAdhoc { .. }
            | Effect::ExecuteExplain { .. }
            | Effect::ExecuteWrite { .. }
            | Effect::CountRowsForExport { .. }
            | Effect::ExportCsv { .. }) => {
                cmd_browse::query::run(
                    e,
                    &self.action_tx,
                    &self.query_executor,
                    &self.query_history_store,
                    state,
                )
                .await?;
                Ok(vec![])
            }

            e @ (Effect::GenerateErDiagramFromCache { .. }
            | Effect::ExtractFkNeighbors { .. }
            | Effect::WriteErFailureLog { .. }
            | Effect::SmartErRefresh { .. }) => {
                cmd_er::run(
                    e,
                    &self.action_tx,
                    &self.metadata_provider,
                    &self.er_exporter,
                    &self.config_writer,
                    &self.er_log_writer,
                    state,
                    completion_engine,
                )
                .await?;
                Ok(vec![])
            }

            e @ Effect::LoadQueryHistory { .. } => {
                cmd_query_history::run(e, &self.action_tx, &self.query_history_store);
                Ok(vec![])
            }

            e @ (Effect::CacheTableInCompletionEngine { .. }
            | Effect::EvictTablesFromCompletionCache { .. }
            | Effect::ClearCompletionEngineCache
            | Effect::ResizeCompletionCache { .. }
            | Effect::TriggerCompletion) => {
                cmd_completion::run(e, &self.action_tx, state, completion_engine).await?;
                Ok(vec![])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cmd::test_support::*;
    use crate::app::ports::RenderOutput;
    use crate::app::ports::connection_store::MockConnectionStore;
    use crate::app::ports::metadata::MockMetadataProvider;
    use crate::app::ports::query_executor::MockQueryExecutor;
    use crate::app::services::AppServices;
    use crate::domain::{DatabaseMetadata, TableSummary};
    use color_eyre::eyre::Result;
    use tokio::sync::mpsc;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(
            &mut self,
            _state: &AppState,
            _services: &AppServices,
            _now: Instant,
        ) -> Result<RenderOutput> {
            Ok(RenderOutput::default())
        }
    }

    mod render {
        use super::*;
        use crate::app::model::browse::jsonb_detail::JsonbDetailState;

        struct ExplorerWidthRenderer {
            explorer_content_width: usize,
        }

        struct JsonbVisibleRowsRenderer {
            visible_rows: usize,
        }

        impl Renderer for ExplorerWidthRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> Result<RenderOutput> {
                Ok(RenderOutput {
                    explorer_content_width: self.explorer_content_width,
                    ..RenderOutput::default()
                })
            }
        }

        impl Renderer for JsonbVisibleRowsRenderer {
            fn draw(
                &mut self,
                _state: &AppState,
                _services: &AppServices,
                _now: Instant,
            ) -> Result<RenderOutput> {
                Ok(RenderOutput {
                    jsonb_detail_editor_visible_rows: Some(self.visible_rows),
                    ..RenderOutput::default()
                })
            }
        }

        #[tokio::test]
        async fn calls_draw() {
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

        #[tokio::test]
        async fn clamps_stale_explorer_horizontal_offset_to_new_maximum() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            state.session.set_metadata(Some(Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                table_summaries: vec![TableSummary::new(
                    "public".to_string(),
                    "abcdefghij".to_string(),
                    Some(0),
                    false,
                )],
                fetched_at: Instant::now(),
            })));
            state.ui.explorer_horizontal_offset = 20;

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = ExplorerWidthRenderer {
                explorer_content_width: 8,
            };

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

            assert_eq!(state.ui.explorer_horizontal_offset, 9);
        }

        #[tokio::test]
        async fn recomputes_jsonb_editor_scroll_when_visible_rows_change() {
            let (tx, _rx) = mpsc::channel(8);
            let runner = make_runner(
                Arc::new(MockMetadataProvider::new()),
                Arc::new(MockQueryExecutor::new()),
                Arc::new(MockConnectionStore::new()),
                TtlCache::new(300),
                tx,
            );

            let state = &mut AppState::new("test".to_string());
            state.jsonb_detail = JsonbDetailState::open_pretty(
                0,
                0,
                "settings".to_string(),
                "{}".to_string(),
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
            );
            state.jsonb_detail.editor_mut().set_content_with_cursor(
                "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}".to_string(),
                29,
            );

            let ce = RefCell::new(CompletionEngine::new());
            let mut renderer = JsonbVisibleRowsRenderer { visible_rows: 2 };

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

            assert_eq!(state.ui.jsonb_detail_editor_visible_rows, 2);
            assert_eq!(state.jsonb_detail.editor().cursor_to_position().0, 3);
            assert_eq!(state.jsonb_detail.editor().scroll_row(), 2);
        }
    }

    mod dispatch_actions {
        use super::*;

        #[tokio::test]
        async fn dispatches_all_actions() {
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

            let result = runner
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

            assert_eq!(result.len(), 2);
            assert!(matches!(result[0], Action::ProcessPrefetchQueue));
            assert!(matches!(result[1], Action::ProcessPrefetchQueue));
        }
    }
}
