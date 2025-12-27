use crate::domain::{DatabaseMetadata, Table};

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        use Action::*;
        match (self, other) {
            (None, None) => true,
            (Quit, Quit) => true,
            (Tick, Tick) => true,
            (Render, Render) => true,
            (Resize(w1, h1), Resize(w2, h2)) => w1 == w2 && h1 == h2,
            (SwitchToBrowse, SwitchToBrowse) => true,
            (SwitchToER, SwitchToER) => true,
            (ToggleFocus, ToggleFocus) => true,
            (Up, Up) => true,
            (Down, Down) => true,
            (Left, Left) => true,
            (Right, Right) => true,
            (OpenTablePicker, OpenTablePicker) => true,
            (CloseTablePicker, CloseTablePicker) => true,
            (OpenCommandPalette, OpenCommandPalette) => true,
            (CloseCommandPalette, CloseCommandPalette) => true,
            (OpenHelp, OpenHelp) => true,
            (CloseHelp, CloseHelp) => true,
            (EnterCommandLine, EnterCommandLine) => true,
            (ExitCommandLine, ExitCommandLine) => true,
            (CommandLineInput(c1), CommandLineInput(c2)) => c1 == c2,
            (CommandLineBackspace, CommandLineBackspace) => true,
            (CommandLineSubmit, CommandLineSubmit) => true,
            (FilterInput(c1), FilterInput(c2)) => c1 == c2,
            (FilterBackspace, FilterBackspace) => true,
            (FilterClear, FilterClear) => true,
            (SelectNext, SelectNext) => true,
            (SelectPrevious, SelectPrevious) => true,
            (SelectFirst, SelectFirst) => true,
            (SelectLast, SelectLast) => true,
            (PageUp, PageUp) => true,
            (PageDown, PageDown) => true,
            (ConfirmSelection, ConfirmSelection) => true,
            (Escape, Escape) => true,
            (LoadMetadata, LoadMetadata) => true,
            (ReloadMetadata, ReloadMetadata) => true,
            (MetadataFailed(e1), MetadataFailed(e2)) => e1 == e2,
            (
                LoadTableDetail {
                    schema: s1,
                    table: t1,
                },
                LoadTableDetail {
                    schema: s2,
                    table: t2,
                },
            ) => s1 == s2 && t1 == t2,
            (TableDetailFailed(e1), TableDetailFailed(e2)) => e1 == e2,
            (InvalidateCache, InvalidateCache) => true,
            // Box<T> variants: compare by pointer for simplicity
            (MetadataLoaded(_), MetadataLoaded(_)) => false,
            (TableDetailLoaded(_), TableDetailLoaded(_)) => false,
            _ => false,
        }
    }
}

impl Eq for Action {}
