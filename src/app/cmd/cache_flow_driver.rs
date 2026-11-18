use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::cmd::er::handler as cmd_er;
use crate::cmd::sql_editor::completion as cmd_completion;
use crate::cmd::test_fixtures::{NoopConfigWriter, NoopErExporter, NoopErLogWriter};
use crate::domain::Table;
use crate::model::app_state::AppState;
use crate::policy::sql::completion::CompletionEngine;
use crate::ports::outbound::{ConfigWriter, ErDiagramExporter, ErLogWriter};
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::reducer::reduce;

const MAX_STEPS: usize = 10_000;

pub struct PendingFetch {
    pub dsn: String,
    pub run_id: u64,
    pub schema: String,
    pub table: String,
}

pub struct CacheFlowDriver {
    pub state: AppState,
    engine: RefCell<CompletionEngine>,
    services: AppServices,
    now: Instant,
    queue: VecDeque<Action>,
    channel: VecDeque<Action>,
    pending_fetches: Vec<PendingFetch>,
    fetched_tables: Vec<String>,
    max_outstanding_fetches: usize,
    io_effects: Vec<Effect>,
}

impl CacheFlowDriver {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            engine: RefCell::new(CompletionEngine::new()),
            services: AppServices::stub(),
            now: Instant::now(),
            queue: VecDeque::new(),
            channel: VecDeque::new(),
            pending_fetches: Vec::new(),
            fetched_tables: Vec::new(),
            max_outstanding_fetches: 0,
            io_effects: Vec::new(),
        }
    }

    pub async fn dispatch(&mut self, action: Action) {
        self.queue.push_back(action);
        self.run_until_idle().await;
    }

    pub async fn complete_fetch(&mut self, qualified_name: &str, table: Table) {
        let index = self
            .pending_fetches
            .iter()
            .position(|fetch| format!("{}.{}", fetch.schema, fetch.table) == qualified_name)
            .unwrap_or_else(|| panic!("no pending fetch for {qualified_name}"));
        let fetch = self.pending_fetches.remove(index);
        self.channel.push_back(Action::TableDetailCached {
            dsn: fetch.dsn,
            run_id: fetch.run_id,
            schema: fetch.schema,
            table: fetch.table,
            detail: Some(Box::new(table)),
        });
        self.run_until_idle().await;
    }

    pub fn seed_cache(&self, table: Table) {
        self.engine
            .borrow_mut()
            .cache_table_detail(table.qualified_name(), table);
    }

    pub fn is_cached(&self, qualified_name: &str) -> bool {
        self.engine.borrow().has_cached_table(qualified_name)
    }

    pub fn pending_fetch_names(&self) -> Vec<String> {
        self.pending_fetches
            .iter()
            .map(|fetch| format!("{}.{}", fetch.schema, fetch.table))
            .collect()
    }

    pub fn fetched_tables(&self) -> &[String] {
        &self.fetched_tables
    }

    pub fn max_outstanding_fetches(&self) -> usize {
        self.max_outstanding_fetches
    }

    pub fn er_generation_count(&self) -> usize {
        self.io_effects
            .iter()
            .filter(|effect| matches!(effect, Effect::GenerateErDiagramFromCache { .. }))
            .count()
    }

    pub fn smart_refresh_run(&self) -> Option<(String, u64)> {
        self.io_effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::SmartErRefresh { dsn, run_id } => Some((dsn.clone(), *run_id)),
                _ => None,
            })
    }

    pub fn metadata_fetch_run(&self) -> Option<u64> {
        self.io_effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::FetchMetadata { run_id, .. } => Some(*run_id),
                _ => None,
            })
    }

    async fn run_until_idle(&mut self) {
        let mut steps = 0;
        loop {
            while let Some(action) = self.queue.pop_front() {
                steps += 1;
                assert!(steps < MAX_STEPS, "action processing did not settle");
                let effects = reduce(&mut self.state, action, self.now, &self.services);
                for effect in effects {
                    self.handle(effect).await;
                }
            }
            match self.channel.pop_front() {
                Some(action) => self.queue.push_back(action),
                None => break,
            }
        }
    }

    async fn handle(&mut self, effect: Effect) {
        match effect {
            Effect::Render
            | Effect::CancelTrackedTasks
            | Effect::CancelMetadataTasks
            | Effect::CancelConnectionTask
            | Effect::CancelSqliteDiagnostics => {}
            Effect::DispatchActions(actions) => self.queue.extend(actions),
            Effect::SchedulePrefetchQueueProcessing { run_id } => {
                self.channel
                    .push_back(Action::ProcessPrefetchQueue { run_id });
            }
            effect @ (Effect::CacheTableInCompletionEngine { .. }
            | Effect::EvictTablesFromCompletionCache { .. }
            | Effect::ClearCompletionEngineCache
            | Effect::ResizeCompletionCache { .. }
            | Effect::TriggerCompletion) => {
                let actions = cmd_completion::run(effect, &self.state, &self.engine);
                self.queue.extend(actions);
            }
            effect @ (Effect::ExtractFkNeighbors { .. }
            | Effect::SmartErRefreshCacheAndDiff { .. }) => {
                let (action_tx, mut action_rx) = mpsc::channel(256);
                let er_exporter: Arc<dyn ErDiagramExporter> = Arc::new(NoopErExporter);
                let config_writer: Arc<dyn ConfigWriter> = Arc::new(NoopConfigWriter);
                let er_log_writer: Arc<dyn ErLogWriter> = Arc::new(NoopErLogWriter);
                cmd_er::run(
                    effect,
                    &action_tx,
                    &er_exporter,
                    &config_writer,
                    &er_log_writer,
                    &self.state,
                    &self.engine,
                )
                .await
                .expect("ER effect should run");
                while let Ok(action) = action_rx.try_recv() {
                    self.channel.push_back(action);
                }
            }
            Effect::PrefetchTableColumnsAndFks {
                dsn,
                run_id,
                schema,
                table,
            } => {
                let qualified_name = format!("{schema}.{table}");
                if self.engine.borrow().has_cached_table(&qualified_name) {
                    self.channel.push_back(Action::TableDetailCached {
                        dsn,
                        run_id,
                        schema,
                        table,
                        detail: None,
                    });
                } else {
                    self.fetched_tables.push(qualified_name);
                    self.pending_fetches.push(PendingFetch {
                        dsn,
                        run_id,
                        schema,
                        table,
                    });
                    self.max_outstanding_fetches =
                        self.max_outstanding_fetches.max(self.pending_fetches.len());
                }
            }
            other => self.io_effects.push(other),
        }
    }
}
