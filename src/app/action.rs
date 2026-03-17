use std::sync::Arc;

use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::focused_pane::FocusedPane;
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

    // Overlay toggles
    OpenTablePicker,
    CloseTablePicker,
    OpenCommandPalette,
    CloseCommandPalette,
    OpenHelp,
    CloseHelp,
    HelpScrollUp,
    HelpScrollDown,

    // Connection lifecycle
    TryConnect,
    SwitchConnection(ConnectionTarget),

    // Connection Setup
    OpenConnectionSetup,
    StartEditConnection(ConnectionId),
    CloseConnectionSetup,
    ConnectionSetupInput(char),
    ConnectionSetupBackspace,
    ConnectionSetupMoveCursor(CursorMove),
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
    ScrollConnectionErrorUp,
    ScrollConnectionErrorDown,
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
    CommandLineInput(char),
    CommandLineBackspace,
    CommandLineSubmit,

    // Filter actions (for Table Picker)
    FilterInput(char),
    FilterBackspace,

    // Navigation
    SelectNext,
    SelectPrevious,
    SelectFirst,
    SelectLast,
    SelectMiddle,
    SelectHalfPageDown,
    SelectHalfPageUp,
    SelectFullPageDown,
    SelectFullPageUp,

    // Explorer horizontal scroll
    ExplorerScrollLeft,
    ExplorerScrollRight,

    // Connection list navigation
    ConnectionListSelectNext,
    ConnectionListSelectPrevious,
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

    // Inspector scroll
    InspectorScrollUp,
    InspectorScrollDown,
    InspectorScrollTop,
    InspectorScrollBottom,
    InspectorScrollMiddle,
    InspectorScrollHalfPageDown,
    InspectorScrollHalfPageUp,
    InspectorScrollFullPageDown,
    InspectorScrollFullPageUp,
    InspectorScrollLeft,
    InspectorScrollRight,

    // Clipboard paste (bracketed paste)
    Paste(String),

    // SQL Modal
    OpenSqlModal,
    CloseSqlModal,
    SqlModalInput(char),
    SqlModalBackspace,
    SqlModalDelete,
    SqlModalNewLine,
    SqlModalTab,
    SqlModalMoveCursor(CursorMove),
    SqlModalSubmit,
    SqlModalClear,
    SqlModalConfirmExecute,
    SqlModalCancelConfirm,
    SqlModalHighRiskInput(char),
    SqlModalHighRiskBackspace,
    SqlModalHighRiskMoveCursor(CursorMove),
    SqlModalHighRiskConfirmExecute,

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
    ResultScrollUp,
    ResultScrollDown,
    ResultScrollTop,
    ResultScrollBottom,
    ResultScrollMiddle,
    ResultScrollHalfPageDown,
    ResultScrollHalfPageUp,
    ResultScrollFullPageDown,
    ResultScrollFullPageUp,
    ResultScrollLeft,
    ResultScrollRight,
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
    ResultCellEditInput(char),
    ResultCellEditBackspace,
    ResultCellEditDelete,
    ResultCellEditMoveCursor(CursorMove),
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

    // Focus mode
    ToggleFocus,

    // Read-only mode
    ToggleReadOnly,

    // ER Table Picker
    OpenErTablePicker,
    CloseErTablePicker,
    ErFilterInput(char),
    ErFilterBackspace,
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
    QueryHistoryFilterInput(char),
    QueryHistoryFilterBackspace,
    QueryHistorySelectNext,
    QueryHistorySelectPrevious,
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
