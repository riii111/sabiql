#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    CommandLine,
    TablePicker,
    CommandPalette,
    Help,
    SqlModal,
}
