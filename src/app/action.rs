use std::sync::Arc;

use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::focused_pane::FocusedPane;
use crate::app::key_sequence::Prefix;
use crate::app::sql_modal_context::CompletionCandidate;
use crate::app::write_guardrails::WritePreview;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use std::collections::HashMap;

use crate::domain::{ConnectionId, DatabaseMetadata, QueryResult, Table};

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
    Explorer,
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
    ResultCellEdit,
    ConnectionSetup,
    CommandLine,
    Filter,
    ErFilter,
    QueryHistoryFilter,
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
    pub error: String,
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
    ConnectionSaveFailed(String),
    ConnectionEditLoaded(Box<ConnectionProfile>),
    ConnectionEditLoadFailed(String),

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
    ConnectionDeleteFailed(String),

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
    MetadataFailed(String),

    // Table detail loading
    LoadTableDetail(TableTarget),
    TableDetailLoaded(Box<Table>, u64),
    TableDetailFailed(String, u64),

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
        error: String,
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
    SqlModalConfirmExecute,
    SqlModalCancelConfirm,
    SqlModalHighRiskConfirmExecute,

    // SQL Modal tabs
    SqlModalNextTab,
    SqlModalPrevTab,

    // EXPLAIN
    ExplainRequest,
    ExplainAnalyzeRequest,
    ExplainCompleted {
        plan_text: String,
        is_analyze: bool,
        execution_time_ms: u64,
    },
    ExplainFailed(String),

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
    QueryFailed(String, u64),
    ExecuteWriteSucceeded {
        affected_rows: usize,
    },
    ExecuteWriteFailed(String),

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
    CopyFailed(String),
    OpenFolderFailed(String),

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
    QueryHistoryLoadFailed(String),
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
    CsvExportFailed(String),

    // ER Diagram (full or partial, depending on selected tables)
    ErOpenDiagram,
    ErGenerateFromCache,
    SmartErRefreshCompleted(SmartErRefreshResult),
    SmartErRefreshFailed(SmartErRefreshError),
    ErDiagramOpened(ErDiagramInfo),
    ErDiagramFailed(String),
    ErLogWriteFailed(String),
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
    }
}
