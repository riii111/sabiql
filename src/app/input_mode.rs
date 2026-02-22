#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    CommandLine,
    CellEdit,
    TablePicker,
    CommandPalette,
    Help,
    SqlModal,
    ConnectionSetup,
    ConnectionError,
    ConfirmDialog,
    ConnectionSelector,
    ErTablePicker,
}
