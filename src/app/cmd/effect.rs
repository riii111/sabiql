use crate::app::update::action::Action;
use crate::domain::Table;
use crate::domain::connection::{ConnectionId, SslMode};

#[derive(Debug, Clone)]
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
    // Updates state.table_detail on completion
    FetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
    },
    // Only caches in completion_engine, does NOT update state.table_detail
    PrefetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
    },
    ProcessPrefetchQueue,
    DelayedProcessPrefetchQueue {
        delay_secs: u64,
    },

    ExecutePreview {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        limit: usize,
        offset: usize,
        target_page: usize,
        read_only: bool,
    },
    ExecuteAdhoc {
        dsn: String,
        query: String,
        read_only: bool,
    },
    ExecuteExplain {
        dsn: String,
        query: String,
        is_analyze: bool,
        read_only: bool,
    },
    ExecuteWrite {
        dsn: String,
        query: String,
        read_only: bool,
    },
    CountRowsForExport {
        dsn: String,
        count_query: String,
        export_query: String,
        file_name: String,
        read_only: bool,
    },
    ExportCsv {
        dsn: String,
        query: String,
        file_name: String,
        row_count: Option<usize>,
        read_only: bool,
    },

    CacheTableInCompletionEngine {
        qualified_name: String,
        table: Box<Table>,
    },
    EvictTablesFromCompletionCache {
        tables: Vec<String>,
    },
    ClearCompletionEngineCache,
    ResizeCompletionCache {
        capacity: usize,
    },
    TriggerCompletion,

    GenerateErDiagramFromCache {
        total_tables: usize,
        project_name: String,
        target_tables: Vec<String>,
    },
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
    },
    ExtractFkNeighbors {
        seed_tables: Vec<String>,
    },
    SmartErRefresh {
        dsn: String,
        run_id: u64,
    },

    CopyToClipboard {
        content: String,
        on_success: Option<Action>,
        on_failure: Option<Action>,
    },
    OpenFolder {
        path: std::path::PathBuf,
    },

    LoadQueryHistory {
        project_name: String,
        connection_id: crate::domain::ConnectionId,
    },

    // Executes effects in order (each awaits before the next),
    // but spawned async tasks (e.g. FetchMetadata) may complete out of order.
    Sequence(Vec<Self>),
    DispatchActions(Vec<Action>),
    SwitchConnection {
        connection_index: usize,
    },
    SwitchToService {
        service_index: usize,
    },
}
