#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    CommandLine,
    CellEdit,
    TablePicker,
    CommandPalette,
    Settings,
    Help,
    SqlModal,
    ConnectionSetup,
    ConnectionError,
    ConfirmDialog,
    ConnectionSelector,
    ErTablePicker,
    QueryHistoryPicker,
    JsonDetail,
    JsonEdit,
    CellDetail,
    SqliteDiagnostics,
    RowDetail,
}

impl InputMode {
    #[deny(
        clippy::wildcard_enum_match_arm,
        reason = "A new mode must decide whether it opens over the panes."
    )]
    pub fn shows_overlay(self) -> bool {
        match self {
            Self::Normal | Self::CommandLine | Self::CellEdit => false,
            Self::TablePicker
            | Self::CommandPalette
            | Self::Settings
            | Self::Help
            | Self::SqlModal
            | Self::ConnectionSetup
            | Self::ConnectionError
            | Self::ConfirmDialog
            | Self::ConnectionSelector
            | Self::ErTablePicker
            | Self::QueryHistoryPicker
            | Self::JsonDetail
            | Self::JsonEdit
            | Self::CellDetail
            | Self::SqliteDiagnostics
            | Self::RowDetail => true,
        }
    }
}
