use std::fmt;
use std::sync::Arc;

use crate::domain::connection::{ConnectionConfig, ConnectionId, DatabaseType};
use crate::domain::query_history::QueryHistoryScope;
use crate::domain::{DatabaseMetadata, QueryValue, Table, TableSignatureSnapshot};
use crate::model::browse::session::ConnectionSaveGuard;
use crate::policy::mask_password;
use crate::ports::outbound::{AccessMode, AppSettings};
use crate::update::action::{Action, ConnectionTarget};

#[derive(Clone)]
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
        dsn: String,
        run_id: u64,
    },
    // Updates state.table_detail on completion
    FetchTableDetail {
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        run_id: u64,
    },
    // Only caches in completion_engine, does NOT update state.table_detail
    PrefetchTableColumnsAndFks {
        dsn: String,
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
        dsn: String,
        schema: String,
        table: String,
        generation: u64,
        run_id: u64,
        limit: usize,
        offset: usize,
        target_page: usize,
    },
    ExecuteAdhoc {
        dsn: String,
        run_id: u64,
        query: String,
        access_mode: AccessMode,
    },
    ExecuteExplain {
        dsn: String,
        database_type: DatabaseType,
        database_generation: u64,
        run_id: u64,
        query: String,
        source_query: String,
        is_analyze: bool,
        access_mode: AccessMode,
    },
    ExecuteWrite {
        dsn: String,
        run_id: u64,
        query: String,
        access_mode: AccessMode,
    },
    CancelConnectionTask,
    CancelMetadataTasks,
    CancelSqliteDiagnostics,
    CancelTrackedTasks,
    ExportCsv {
        dsn: String,
        run_id: u64,
        query: String,
        file_name: String,
    },
    ExportCsvFromCache {
        dsn: String,
        run_id: u64,
        file_name: String,
        columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
        row_count: Option<usize>,
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
        run_id: u64,
        total_tables: usize,
        project_name: String,
        target_tables: Vec<String>,
    },
    WriteErFailureLog {
        failed_tables: Vec<(String, String)>,
    },
    ExtractFkNeighbors {
        run_id: u64,
        seed_tables: Vec<String>,
    },
    SmartErRefresh {
        dsn: String,
        run_id: u64,
    },
    SmartErRefreshCacheAndDiff {
        dsn: String,
        run_id: u64,
        new_metadata: Arc<DatabaseMetadata>,
        signature_snapshot: Arc<TableSignatureSnapshot>,
    },

    CopyToClipboard {
        content: String,
        on_success: Box<Action>,
        on_failure: Option<Box<Action>>,
    },
    OpenFolder {
        path: std::path::PathBuf,
        message_revision: u64,
        export_message: String,
    },

    LoadQueryHistory {
        project_name: String,
        scope: QueryHistoryScope,
    },

    SaveSettings {
        settings: AppSettings,
    },

    FetchSqliteDiagnosticsCore {
        dsn: String,
        run_id: u64,
    },

    FetchSqliteDiagnosticsQuickCheck {
        dsn: String,
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

struct MaskedDsn<'a>(&'a str);

impl fmt::Debug for MaskedDsn<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&mask_password(self.0))
    }
}

impl fmt::Debug for Effect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render => formatter.write_str("Effect::Render"),
            Self::SaveAndConnect {
                id,
                name,
                run_id,
                run_guard,
                ..
            } => formatter
                .debug_struct("Effect::SaveAndConnect")
                .field("id", id)
                .field("name", name)
                .field("config", &"<redacted>")
                .field("run_id", run_id)
                .field("run_guard", run_guard)
                .finish(),
            Self::ProbeMySqlConnection { target, run_id } => formatter
                .debug_struct("Effect::ProbeMySqlConnection")
                .field("target", target)
                .field("run_id", run_id)
                .finish(),
            Self::LoadConnectionForEdit { id } => formatter
                .debug_struct("Effect::LoadConnectionForEdit")
                .field("id", id)
                .finish(),
            Self::LoadConnections => formatter.write_str("Effect::LoadConnections"),
            Self::DeleteConnection { id } => formatter
                .debug_struct("Effect::DeleteConnection")
                .field("id", id)
                .finish(),
            Self::FetchMetadata { dsn, run_id } => formatter
                .debug_struct("Effect::FetchMetadata")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .finish(),
            Self::FetchTableDetail {
                dsn,
                schema,
                table,
                generation,
                run_id,
            } => formatter
                .debug_struct("Effect::FetchTableDetail")
                .field("dsn", &MaskedDsn(dsn))
                .field("schema", schema)
                .field("table", table)
                .field("generation", generation)
                .field("run_id", run_id)
                .finish(),
            Self::PrefetchTableColumnsAndFks {
                dsn,
                run_id,
                schema,
                table,
            } => formatter
                .debug_struct("Effect::PrefetchTableColumnsAndFks")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("schema", schema)
                .field("table", table)
                .finish(),
            Self::SchedulePrefetchQueueProcessing { run_id } => formatter
                .debug_struct("Effect::SchedulePrefetchQueueProcessing")
                .field("run_id", run_id)
                .finish(),
            Self::DelayedProcessPrefetchQueue { run_id, delay_secs } => formatter
                .debug_struct("Effect::DelayedProcessPrefetchQueue")
                .field("run_id", run_id)
                .field("delay_secs", delay_secs)
                .finish(),
            Self::ExecutePreview {
                dsn,
                schema,
                table,
                generation,
                run_id,
                limit,
                offset,
                target_page,
            } => formatter
                .debug_struct("Effect::ExecutePreview")
                .field("dsn", &MaskedDsn(dsn))
                .field("schema", schema)
                .field("table", table)
                .field("generation", generation)
                .field("run_id", run_id)
                .field("limit", limit)
                .field("offset", offset)
                .field("target_page", target_page)
                .finish(),
            Self::ExecuteAdhoc {
                dsn,
                run_id,
                query,
                access_mode,
            } => formatter
                .debug_struct("Effect::ExecuteAdhoc")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("query", query)
                .field("access_mode", access_mode)
                .finish(),
            Self::ExecuteExplain {
                dsn,
                database_type,
                database_generation,
                run_id,
                query,
                source_query,
                is_analyze,
                access_mode,
            } => formatter
                .debug_struct("Effect::ExecuteExplain")
                .field("dsn", &MaskedDsn(dsn))
                .field("database_type", database_type)
                .field("database_generation", database_generation)
                .field("run_id", run_id)
                .field("query", query)
                .field("source_query", source_query)
                .field("is_analyze", is_analyze)
                .field("access_mode", access_mode)
                .finish(),
            Self::ExecuteWrite {
                dsn,
                run_id,
                query,
                access_mode,
            } => formatter
                .debug_struct("Effect::ExecuteWrite")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("query", query)
                .field("access_mode", access_mode)
                .finish(),
            Self::CancelConnectionTask => formatter.write_str("Effect::CancelConnectionTask"),
            Self::CancelMetadataTasks => formatter.write_str("Effect::CancelMetadataTasks"),
            Self::CancelSqliteDiagnostics => formatter.write_str("Effect::CancelSqliteDiagnostics"),
            Self::CancelTrackedTasks => formatter.write_str("Effect::CancelTrackedTasks"),
            Self::ExportCsv {
                dsn,
                run_id,
                query,
                file_name,
            } => formatter
                .debug_struct("Effect::ExportCsv")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("query", query)
                .field("file_name", file_name)
                .finish(),
            Self::ExportCsvFromCache {
                dsn,
                run_id,
                file_name,
                columns,
                values,
                row_count,
            } => formatter
                .debug_struct("Effect::ExportCsvFromCache")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("file_name", file_name)
                .field("columns", columns)
                .field("values", values)
                .field("row_count", row_count)
                .finish(),
            Self::CacheTableInCompletionEngine {
                qualified_name,
                table,
            } => formatter
                .debug_struct("Effect::CacheTableInCompletionEngine")
                .field("qualified_name", qualified_name)
                .field("table", table)
                .finish(),
            Self::EvictTablesFromCompletionCache { tables } => formatter
                .debug_struct("Effect::EvictTablesFromCompletionCache")
                .field("tables", tables)
                .finish(),
            Self::ClearCompletionEngineCache => {
                formatter.write_str("Effect::ClearCompletionEngineCache")
            }
            Self::ResizeCompletionCache { capacity } => formatter
                .debug_struct("Effect::ResizeCompletionCache")
                .field("capacity", capacity)
                .finish(),
            Self::TriggerCompletion => formatter.write_str("Effect::TriggerCompletion"),
            Self::GenerateErDiagramFromCache {
                run_id,
                total_tables,
                project_name,
                target_tables,
            } => formatter
                .debug_struct("Effect::GenerateErDiagramFromCache")
                .field("run_id", run_id)
                .field("total_tables", total_tables)
                .field("project_name", project_name)
                .field("target_tables", target_tables)
                .finish(),
            Self::WriteErFailureLog { failed_tables } => formatter
                .debug_struct("Effect::WriteErFailureLog")
                .field("failed_tables", failed_tables)
                .finish(),
            Self::ExtractFkNeighbors {
                run_id,
                seed_tables,
            } => formatter
                .debug_struct("Effect::ExtractFkNeighbors")
                .field("run_id", run_id)
                .field("seed_tables", seed_tables)
                .finish(),
            Self::SmartErRefresh { dsn, run_id } => formatter
                .debug_struct("Effect::SmartErRefresh")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .finish(),
            Self::SmartErRefreshCacheAndDiff {
                dsn,
                run_id,
                new_metadata,
                signature_snapshot,
            } => formatter
                .debug_struct("Effect::SmartErRefreshCacheAndDiff")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("new_metadata", new_metadata)
                .field("signature_snapshot", signature_snapshot)
                .finish(),
            Self::CopyToClipboard {
                content,
                on_success,
                on_failure,
            } => formatter
                .debug_struct("Effect::CopyToClipboard")
                .field("content", &mask_password(content))
                .field("on_success", on_success)
                .field("on_failure", on_failure)
                .finish(),
            Self::OpenFolder {
                path,
                message_revision,
                export_message,
            } => formatter
                .debug_struct("Effect::OpenFolder")
                .field("path", path)
                .field("message_revision", message_revision)
                .field("export_message", export_message)
                .finish(),
            Self::LoadQueryHistory {
                project_name,
                scope,
            } => formatter
                .debug_struct("Effect::LoadQueryHistory")
                .field("project_name", project_name)
                .field("scope", scope)
                .finish(),
            Self::SaveSettings { settings } => formatter
                .debug_struct("Effect::SaveSettings")
                .field("settings", settings)
                .finish(),
            Self::FetchSqliteDiagnosticsCore { dsn, run_id } => formatter
                .debug_struct("Effect::FetchSqliteDiagnosticsCore")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .finish(),
            Self::FetchSqliteDiagnosticsQuickCheck { dsn, run_id } => formatter
                .debug_struct("Effect::FetchSqliteDiagnosticsQuickCheck")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .finish(),
            Self::DispatchActions(actions) => formatter
                .debug_struct("Effect::DispatchActions")
                .field("action_count", &actions.len())
                .finish(),
            Self::SwitchConnection { connection_index } => formatter
                .debug_struct("Effect::SwitchConnection")
                .field("connection_index", connection_index)
                .finish(),
            Self::SwitchToService { service_index } => formatter
                .debug_struct("Effect::SwitchToService")
                .field("service_index", service_index)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::browse::session::BrowseSession;
    use crate::model::connection::cache::ConnectionCache;

    #[test]
    fn debug_masks_uri_passwords_in_startup_effect_session_and_cache() {
        let dsn = "postgresql://user@host/db?pass%77ord=secret";
        let effect = Effect::FetchMetadata {
            dsn: dsn.to_string(),
            run_id: 1,
        };
        let mut session = BrowseSession::default();
        session.activate_cli_ephemeral_connection_with_target(
            &ConnectionId::from_string("cli-uri-test"),
            "host",
            DatabaseType::PostgreSQL,
            dsn,
            None,
        );
        let cache = ConnectionCache {
            connection_dsn: Some(dsn.to_string()),
            ..Default::default()
        };

        for debug in [
            format!("{effect:?}"),
            format!("{session:?}"),
            format!("{cache:?}"),
        ] {
            assert!(!debug.contains("secret"));
            assert!(debug.contains("pass%77ord=****"));
        }
    }
}
