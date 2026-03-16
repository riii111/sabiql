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
    DisableReadOnly,
}

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub intent: Option<ConfirmIntent>,
}

impl ConfirmDialogState {
    pub fn open(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        intent: ConfirmIntent,
    ) {
        self.title = title.into();
        self.message = message.into();
        self.intent = Some(intent);
    }
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: String::new(),
            intent: None,
        }
    }
}
