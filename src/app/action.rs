use crate::domain::{DatabaseMetadata, Table};

#[derive(Debug, Clone)]
pub enum Action {
    None,
    Quit,
    Tick,
    Render,
    Resize(u16, u16),
    SwitchToBrowse,
    SwitchToER,
    ToggleFocus,
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
    LoadTableDetail { schema: String, table: String },
    TableDetailLoaded(Box<Table>),
    TableDetailFailed(String),

    // Cache operations
    InvalidateCache,
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
    }
}
