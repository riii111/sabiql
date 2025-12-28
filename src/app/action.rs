use crate::app::focused_pane::FocusedPane;
use crate::app::inspector_tab::InspectorTab;
use crate::domain::{DatabaseMetadata, QueryResult, Table};

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
    Tick,
    Render,
    Resize(u16, u16),
    NextTab,
    PreviousTab,
    SetFocusedPane(FocusedPane),
    Up,
    Down,
    Left,
    Right,

    // Overlay toggles
    OpenTablePicker,
    CloseTablePicker,
    OpenCommandPalette,
    CloseCommandPalette,
    OpenHelp,
    CloseHelp,

    // Command line actions
    EnterCommandLine,
    ExitCommandLine,
    CommandLineInput(char),
    CommandLineBackspace,
    CommandLineSubmit,

    // Filter actions (for Table Picker)
    FilterInput(char),
    FilterBackspace,
    FilterClear,

    // Navigation
    SelectNext,
    SelectPrevious,
    SelectFirst,
    SelectLast,
    PageUp,
    PageDown,

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

    // Cache operations
    InvalidateCache,

    // Inspector sub-tabs
    InspectorNextTab,
    InspectorPrevTab,
    InspectorSelectTab(InspectorTab),

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

    // Query execution
    ExecutePreview {
        schema: String,
        table: String,
        generation: u64,
    },
    ExecuteAdhoc(String),
    QueryCompleted(Box<QueryResult>, u64),
    QueryFailed(String, u64),

    // Result pane
    ResultScrollUp,
    ResultScrollDown,
    ResultScrollTop,
    ResultScrollBottom,
    HistoryPrev,
    HistoryNext,

    // Clipboard
    CopySelection,
    CopyLastError,
    CopyToClipboard(String),
    ClipboardSuccess,
    ClipboardFailed(String),

    // Console
    OpenConsole,
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
    }
}
