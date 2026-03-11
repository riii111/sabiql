use crate::app::input_mode::InputMode;
use crate::domain::ConnectionId;

#[derive(Debug, Clone)]
pub enum ConfirmIntent {
    QuitNoConnection,
    DeleteConnection(ConnectionId),
    /// blocked=true disables the confirm button in UI
    ExecuteWrite {
        sql: String,
        blocked: bool,
    },
    CsvExport {
        export_query: String,
        file_name: String,
        row_count: Option<usize>,
    },
}

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub intent: Option<ConfirmIntent>,
    pub return_mode: InputMode,
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: String::new(),
            intent: None,
            return_mode: InputMode::Normal,
        }
    }
}
