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
    ErDiagram,
    QueryHistoryPicker,
    JsonDetail,
    JsonEdit,
    CellDetail,
    SqliteDiagnostics,
    RowDetail,
}
