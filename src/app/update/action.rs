use std::sync::Arc;

use crate::app::model::connection::error::ConnectionErrorInfo;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::key_sequence::Prefix;
use crate::app::model::sql_editor::completion::CompletionCandidate;
use crate::app::policy::write::write_guardrails::WritePreview;
use crate::app::ports::DbOperationError;
use crate::app::ports::clipboard::ClipboardError;
use crate::app::ports::connection_store::ConnectionStoreError;
use crate::app::ports::folder_opener::FolderOpenError;
use crate::app::ports::query_history::QueryHistoryError;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use std::collections::HashMap;

use crate::domain::{ConnectionId, DatabaseMetadata, QueryResult, Table};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionSaveError {
    #[error("{0}")]
    Validation(#[from] crate::domain::connection::ConnectionNameError),
    #[error("{0}")]
    Store(#[from] ConnectionStoreError),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ErDiagramError {
    #[error("{0}")]
    NoData(String),
    #[error("{0}")]
    ExportFailed(String),
    #[error("Task panicked: {0}")]
    TaskPanicked(String),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ErLogError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Config(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

// ---------------------------------------------------------------------------
// Parametric Action types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Result,
    Inspector,
    Help,
    ConnectionError,
    ExplainPlan,
    ExplainCompare,
    ExplainConfirm,
    Explorer,
    JsonbDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAmount {
    Line,
    ToStart,
    ToEnd,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    HalfPage,
    FullPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollToCursorTarget {
    Explorer,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    Center,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    SqlModal,
    SqlModalHighRisk,
    SqlModalAnalyzeHighRisk,
    ResultCellEdit,
    ConnectionSetup,
    CommandLine,
    Filter,
    ErFilter,
    QueryHistoryFilter,
    JsonbEdit,
    JsonbSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMotion {
    Next,
    Previous,
    First,
    Last,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    HalfPageDown,
    HalfPageUp,
    FullPageDown,
    FullPageUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTarget {
    ConnectionList,
    QueryHistory,
    TablePicker,
    ErTablePicker,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMotion {
    Next,
    Previous,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshResult {
    pub run_id: u64,
    pub new_metadata: Arc<DatabaseMetadata>,
    pub stale_tables: Vec<String>,
    pub added_tables: Vec<String>,
    pub removed_tables: Vec<String>,
    pub missing_in_cache: Vec<String>,
    pub new_signatures: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshError {
    pub run_id: u64,
    pub error: DbOperationError,
    pub new_metadata: Option<Arc<DatabaseMetadata>>,
}

#[derive(Debug, Clone)]
pub struct ErDiagramInfo {
    pub path: String,
    pub table_count: usize,
    pub total_tables: usize,
}

#[derive(Debug, Clone)]
pub struct ConnectionsLoadedPayload {
    pub profiles: Vec<ConnectionProfile>,
    pub services: Vec<ServiceEntry>,
    pub service_file_path: Option<std::path::PathBuf>,
    pub profile_load_warning: Option<String>,
    pub service_load_warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TableTarget {
    pub schema: String,
    pub table: String,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct ConnectionTarget {
    pub id: ConnectionId,
    pub dsn: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum Action {
    None,
    Quit,
    Render,
    Resize(u16, u16),
    SetFocusedPane(FocusedPane),

    // Parametric variants (consolidation targets)
    Scroll {
        target: ScrollTarget,
        direction: ScrollDirection,
        amount: ScrollAmount,
    },
    ScrollToCursor {
        target: ScrollToCursorTarget,
        position: CursorPosition,
    },
    TextInput {
        target: InputTarget,
        ch: char,
    },
    TextBackspace {
        target: InputTarget,
    },
    TextDelete {
        target: InputTarget,
    },
    TextMoveCursor {
        target: InputTarget,
        direction: CursorMove,
    },
    Select(SelectMotion),
    ListSelect {
        target: ListTarget,
        motion: ListMotion,
    },

    // Overlay toggles
    OpenTablePicker,
    CloseTablePicker,
    OpenCommandPalette,
    CloseCommandPalette,
    OpenHelp,
    CloseHelp,

    // Connection lifecycle
    TryConnect,
    SwitchConnection(ConnectionTarget),

    // Connection Setup
    OpenConnectionSetup,
    StartEditConnection(ConnectionId),
    CloseConnectionSetup,
    ConnectionSetupNextField,
    ConnectionSetupPrevField,
    ConnectionSetupToggleDropdown,
    ConnectionSetupDropdownNext,
    ConnectionSetupDropdownPrev,
    ConnectionSetupDropdownConfirm,
    ConnectionSetupDropdownCancel,
    ConnectionSetupSave,
    ConnectionSetupCancel,
    ConnectionSaveCompleted(ConnectionTarget),
    ConnectionSaveFailed(ConnectionSaveError),
    ConnectionEditLoaded(Box<ConnectionProfile>),
    ConnectionEditLoadFailed(ConnectionStoreError),

    // Connection Selector
    OpenConnectionSelector,

    // Connection Error
    ShowConnectionError(ConnectionErrorInfo),
    CloseConnectionError,
    ToggleConnectionErrorDetails,
    CopyConnectionError,
    ConnectionErrorCopied,
    ReenterConnectionSetup,
    RetryServiceConnection,

    // Confirm Dialog
    ConfirmDialogConfirm,
    ConfirmDialogCancel,

    // Connection deletion
    RequestDeleteSelectedConnection,
    DeleteConnection(ConnectionId),
    ConnectionDeleted(ConnectionId),
    ConnectionDeleteFailed(ConnectionStoreError),

    // Connection edit (from list)
    RequestEditSelectedConnection,

    // Command line actions
    EnterCommandLine,
    ExitCommandLine,
    CommandLineSubmit,

    // Connection list navigation
    ConnectionsLoaded(ConnectionsLoadedPayload),
    ConfirmConnectionSelection,

    // Selection
    ConfirmSelection,

    // Escape (context-dependent close)
    Escape,

    // Metadata loading
    LoadMetadata,
    ReloadMetadata,
    MetadataLoaded(Arc<DatabaseMetadata>),
    MetadataFailed(DbOperationError),

    // Table detail loading
    LoadTableDetail(TableTarget),
    TableDetailLoaded(Box<Table>, u64),
    TableDetailFailed(DbOperationError, u64),

    // Completion prefetch (does NOT update state.table_detail)
    PrefetchTableDetail {
        schema: String,
        table: String,
    },
    TableDetailCached {
        schema: String,
        table: String,
        detail: Box<Table>,
    },
    TableDetailCacheFailed {
        schema: String,
        table: String,
        error: DbOperationError,
    },
    TableDetailAlreadyCached {
        schema: String,
        table: String,
    },

    // Prefetch all tables for completion
    StartPrefetchAll,
    StartPrefetchScoped {
        tables: Vec<String>,
    },
    ExpandPrefetchWithFkNeighbors,
    FkNeighborsDiscovered {
        tables: Vec<String>,
    },
    ProcessPrefetchQueue,

    // Inspector sub-tabs
    InspectorNextTab,
    InspectorPrevTab,

    // Clipboard paste (bracketed paste)
    Paste(String),

    // SQL Modal
    OpenSqlModal,
    CloseSqlModal,
    SqlModalEnterInsert,
    SqlModalEnterNormal,
    SqlModalYank,
    SqlModalYankSuccess,
    SqlModalNewLine,
    SqlModalTab,
    SqlModalSubmit,
    SqlModalClear,
    SqlModalCancelConfirm,
    SqlModalHighRiskConfirmExecute,

    // SQL Modal tabs
    SqlModalNextTab,
    SqlModalPrevTab,

    // EXPLAIN
    ExplainRequest,
    ExplainAnalyzeRequest,
    ExplainAnalyzeConfirm,
    ExplainAnalyzeCancel,
    ExplainCompleted {
        plan_text: String,
        is_analyze: bool,
        execution_time_ms: u64,
    },
    ExplainFailed(DbOperationError),
    CompareEditQuery,

    // SQL Modal completion
    CompletionTrigger,
    CompletionUpdated {
        candidates: Vec<CompletionCandidate>,
        trigger_position: usize,
        visible: bool,
    },
    CompletionAccept,
    CompletionDismiss,
    CompletionNext,
    CompletionPrev,

    // Query execution
    ExecutePreview(TableTarget),
    ExecuteAdhoc(String),
    ExecuteWrite(String),
    QueryCompleted {
        result: Arc<QueryResult>,
        generation: u64,
        target_page: Option<usize>,
    },
    QueryFailed(DbOperationError, u64),
    ExecuteWriteSucceeded {
        affected_rows: usize,
    },
    ExecuteWriteFailed(DbOperationError),

    // Result pane
    ResultNextPage,
    ResultPrevPage,

    // Result pane selection
    ResultEnterRowActive,
    ResultEnterCellActive,
    ResultExitToRowActive,
    ResultExitToScroll,
    ResultCellLeft,
    ResultCellRight,
    ResultCellYank,
    ResultRowYankOperatorPending,
    ResultRowYank,
    DdlYank,
    ResultDeleteOperatorPending,
    StageRowForDelete,
    UnstageLastStagedRow,
    ClearStagedDeletes,
    RequestDeleteActiveRow,
    ResultEnterCellEdit,
    ResultCancelCellEdit,
    ResultDiscardCellEdit,
    SubmitCellEditWrite,
    OpenWritePreviewConfirm(Box<WritePreview>),
    CellCopied,
    CopyFailed(ClipboardError),
    OpenFolderFailed(FolderOpenError),

    // Result history navigation
    OpenResultHistory,
    HistoryOlder,
    HistoryNewer,
    ExitResultHistory,

    // Multi-key sequence FSM (zz, zt, zb)
    BeginKeySequence(Prefix),
    CancelKeySequence,

    // Focus mode
    ToggleFocus,

    // Read-only mode
    ToggleReadOnly,

    // ER Table Picker
    OpenErTablePicker,
    CloseErTablePicker,
    ErToggleSelection,
    ErSelectAll,
    ErConfirmSelection,

    // Query History Picker
    OpenQueryHistoryPicker,
    CloseQueryHistoryPicker,
    QueryHistoryLoaded(
        crate::domain::ConnectionId,
        Vec<crate::domain::query_history::QueryHistoryEntry>,
    ),
    QueryHistoryLoadFailed(QueryHistoryError),
    QueryHistoryAppendFailed(QueryHistoryError),
    QueryHistoryConfirmSelection,

    // CSV Export
    RequestCsvExport,
    CsvExportRowsCounted {
        row_count: Option<usize>,
        export_query: String,
        file_name: String,
    },
    ExecuteCsvExport {
        export_query: String,
        file_name: String,
        row_count: Option<usize>,
    },
    CsvExportSucceeded {
        path: String,
        row_count: Option<usize>,
    },
    CsvExportFailed(DbOperationError),

    // JSONB Detail View
    OpenJsonbDetail,
    CloseJsonbDetail,
    JsonbCursorUp,
    JsonbCursorDown,
    JsonbScrollToTop,
    JsonbScrollToEnd,
    JsonbToggleFold,
    JsonbFoldAll,
    JsonbUnfoldAll,
    JsonbYankAll,
    JsonbEnterEdit,
    JsonbExitEdit,
    JsonbEnterSearch,
    JsonbExitSearch,
    JsonbSearchNext,
    JsonbSearchPrev,
    JsonbSearchSubmit,

    // ER Diagram (full or partial, depending on selected tables)
    ErOpenDiagram,
    ErGenerateFromCache,
    SmartErRefreshCompleted(SmartErRefreshResult),
    SmartErRefreshFailed(SmartErRefreshError),
    ErDiagramOpened(ErDiagramInfo),
    ErDiagramFailed(ErDiagramError),
    ErLogWriteFailed(ErLogError),
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn scroll_action_returns_true() {
        let action = Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        };
        assert!(action.is_scroll());
    }

    #[rstest]
    #[case(Action::None)]
    #[case(Action::Quit)]
    #[case(Action::Render)]
    #[case(Action::ScrollToCursor {
        target: ScrollToCursorTarget::Result,
        position: CursorPosition::Center,
    })]
    fn non_scroll_action_returns_false(#[case] action: Action) {
        assert!(!action.is_scroll());
    }
}
