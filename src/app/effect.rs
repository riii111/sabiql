//! Side effects returned by the reducer, executed by EffectRunner.

use crate::app::action::Action;
use crate::domain::Table;
use crate::domain::connection::{ConnectionId, SslMode};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Effect {
    Render,

    SaveAndConnect {
        id: Option<ConnectionId>,
        name: String,
        host: String,
        port: u16,
        database: String,
        user: String,
        password: String,
        ssl_mode: SslMode,
    },

    LoadConnectionForEdit {
        id: ConnectionId,
    },

    LoadConnections,

    DeleteConnection {
        id: ConnectionId,
    },

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
        offset: usize,
        target_page: usize,
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

    CopyToClipboard {
        content: String,
    },

    GenerateErDiagramFromCache {
        total_tables: usize,
        project_name: String,
        target_tables: Vec<String>,
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
