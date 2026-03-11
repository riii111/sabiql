use crate::app::input_mode::InputMode;
use crate::domain::ConnectionId;

/// Typed intent for confirm dialogs.
/// Each variant represents a specific workflow that requires user confirmation.
#[derive(Debug, Clone)]
pub enum ConfirmIntent {
    /// First-run quit confirmation when no connection is configured.
    /// confirm → quit, cancel → return to ConnectionSetup
    QuitNoConnection,
    /// Connection profile deletion.
    DeleteConnection(ConnectionId),
    /// Write (UPDATE/DELETE) preview execution confirmation.
    /// blocked=true means confirm is a no-op (UI hides the button).
    ExecuteWrite { sql: String, blocked: bool },
    /// Large CSV export confirmation.
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
    /// The InputMode to return to after dialog closes
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
