use std::sync::Arc;

use crate::domain::connection::{ConnectionConfig, ConnectionEndpoint, ConnectionId};
use crate::domain::query_history::QueryHistoryScope;
use crate::domain::{ErTableInfo, QueryResult};
use crate::model::browse::session::ConnectionSaveGuard;
use crate::ports::outbound::{AccessMode, AppSettings};
use crate::update::action::{ConnectionTarget, CopyTarget};

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
        tables: Vec<(String, String)>,
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
    LoadTable {
        endpoint: Arc<ConnectionEndpoint>,
        schema: String,
        table: String,
        generation: u64,
        detail_run_id: u64,
        preview_run_id: u64,
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
    // A smart ER refresh reads metadata through the same provider, so this also stops it.
    CancelMetadataTasks,
    CancelTableDetail,
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
    SmartErRefresh {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
    },

    CopyToClipboard {
        content: String,
        target: CopyTarget,
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
}
