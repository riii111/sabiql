use std::sync::Arc;

use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::explorer_mode::ExplorerMode;
use crate::app::focused_pane::FocusedPane;
use crate::app::sql_modal_context::CompletionCandidate;
use crate::domain::connection::ConnectionProfile;
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
#[allow(dead_code)]
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
    SwitchConnection {
        id: ConnectionId,
        dsn: String,
        name: String,
    },

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
    ConnectionSaveCompleted {
        id: ConnectionId,
        dsn: String,
        name: String,
    },
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

    // Confirm Dialog
    OpenConfirmDialog,
    CloseConfirmDialog,
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

    // Explorer horizontal scroll
    ExplorerScrollLeft,
    ExplorerScrollRight,

    // Explorer mode (Tables / Connections)
    ToggleExplorerMode,
    SetExplorerMode(ExplorerMode),
    ConnectionListSelectNext,
    ConnectionListSelectPrevious,
    ConnectionsLoaded(Vec<ConnectionProfile>),
    ConfirmConnectionSelection,

    // Selection
    ConfirmSelection,

    // Escape (context-dependent close)
    Escape,

    // Metadata loading
    LoadMetadata,
    ReloadMetadata,
    MetadataLoaded(Box<DatabaseMetadata>),
    MetadataFailed(String),

    // Table detail loading
    LoadTableDetail {
        schema: String,
        table: String,
        generation: u64,
    },
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
    /// Notifies that table was already cached (no data update needed)
    TableDetailAlreadyCached {
        schema: String,
        table: String,
    },

    // Prefetch all tables for completion
    StartPrefetchAll,
    ProcessPrefetchQueue,

    // Inspector sub-tabs
    InspectorNextTab,
    InspectorPrevTab,

    // Inspector scroll
    InspectorScrollUp,
    InspectorScrollDown,
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
    ExecutePreview {
        schema: String,
        table: String,
        generation: u64,
    },
    ExecuteAdhoc(String),
    QueryCompleted(Arc<QueryResult>, u64),
    QueryFailed(String, u64),

    // Result pane
    ResultScrollUp,
    ResultScrollDown,
    ResultScrollTop,
    ResultScrollBottom,
    ResultScrollLeft,
    ResultScrollRight,

    // Focus mode
    ToggleFocus,

    // ER Diagram (full database diagram)
    ErOpenDiagram,
    ErDiagramOpened {
        path: String,
        table_count: usize,
        total_tables: usize,
    },
    ErDiagramFailed(String),
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
    }
}
