use std::sync::Arc;

use crate::domain::connection::{ConnectionConfig, ConnectionEndpoint, ConnectionId};
use crate::domain::query_history::QueryHistoryScope;
use crate::domain::{ErTableInfo, QueryResult};
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::{AccessMode, AppSettings};
use crate::update::action::{Action, ConnectionTarget};

#[derive(Debug, Clone)]
pub enum Effect {
    Render,

    SaveAndConnect {
        id: Option<ConnectionId>,
        name: String,
        config: ConnectionConfig,
        run_id: u64,
        run_guard: Arc<ConnectionSaveGuard>,
    },
    ProbeMySqlConnection {
        target: ConnectionTarget,
        run_id: u64,
    },
    LoadConnectionForEdit {
        id: ConnectionId,
    },
    LoadConnections,
    DeleteConnection {
        id: ConnectionId,
    },

    FetchMetadata {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },
    FetchEffectiveUser {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },
    // Updates state.table_detail on completion
    FetchTableDetail {
        endpoint: Arc<ConnectionEndpoint>,
        schema: String,
        table: String,
        generation: u64,
        run_id: u64,
    },
    // Only fills the table detail cache, does NOT update state.table_detail
    PrefetchTableColumnsAndFks {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        schema: String,
        table: String,
    },
    SchedulePrefetchQueueProcessing {
        run_id: u64,
    },
    DelayedProcessPrefetchQueue {
        run_id: u64,
        delay_secs: u64,
    },

    ExecutePreview {
        endpoint: Arc<ConnectionEndpoint>,
        schema: String,
        table: String,
        generation: u64,
        run_id: u64,
        limit: usize,
        offset: usize,
        target_page: usize,
    },
    ExecuteAdhoc {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        query: String,
        access_mode: AccessMode,
    },
    ExecuteExplain {
        endpoint: Arc<ConnectionEndpoint>,
        database_generation: u64,
        run_id: u64,
        query: String,
        source_query: String,
        is_analyze: bool,
        access_mode: AccessMode,
    },
    ExecuteWrite {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        query: String,
        access_mode: AccessMode,
    },
    CancelConnectionTask,
    CancelMetadataTasks,
    CancelSqliteDiagnostics,
    CancelTrackedTasks,
    ExportCsvFromCache {
        run_id: u64,
        file_name: String,
        result: Arc<QueryResult>,
    },

    GenerateErDiagram {
        run_id: u64,
        total_tables: usize,
        project_name: String,
        tables: Vec<ErTableInfo>,
        filename: String,
    },
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
    },
    SmartErRefresh {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },

    CopyToClipboard {
        content: String,
        on_success: Box<Action>,
        on_failure: Option<Box<Action>>,
    },
    OpenFolder {
        path: std::path::PathBuf,
    },

    LoadQueryHistory {
        project_name: String,
        scope: QueryHistoryScope,
    },

    SaveSettings {
        settings: AppSettings,
    },

    FetchSqliteDiagnosticsCore {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },

    FetchSqliteDiagnosticsQuickCheck {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },

    DispatchActions(Vec<Action>),
    SwitchConnection {
        connection_index: usize,
    },
    SwitchToService {
        service_index: usize,
    },
}
