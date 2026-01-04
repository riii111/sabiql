//! Side effects returned by the reducer, executed by EffectRunner.

use crate::app::action::Action;
use crate::domain::Table;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Effect {
    Render,

    CacheInvalidate {
        dsn: String,
    },

    FetchMetadata {
        dsn: String,
    },

    /// Updates state.table_detail on completion
    FetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
    },
    /// Only caches in completion_engine, does NOT update state.table_detail
    PrefetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
    },
    ProcessPrefetchQueue,

    ExecutePreview {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        limit: usize,
    },
    ExecuteAdhoc {
        dsn: String,
        query: String,
    },

    CacheTableInCompletionEngine {
        qualified_name: String,
        table: Box<Table>,
    },
    ClearCompletionEngineCache,

    GenerateErDiagramFromCache {
        total_tables: usize,
        project_name: String,
    },
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
    },

    /// Triggers completion: fetches missing tables and updates candidates
    TriggerCompletion,

    /// Executes effects in order. Each effect awaits before starting the next,
    /// but spawned async tasks (e.g., FetchMetadata) may complete out of order.
    Sequence(Vec<Effect>),

    /// Dispatch actions to be processed by the reducer
    DispatchActions(Vec<Action>),
}

#[allow(dead_code)]
impl Effect {
    pub fn is_render(&self) -> bool {
        matches!(self, Effect::Render)
    }
}
