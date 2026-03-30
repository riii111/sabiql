use crate::domain::ConnectionId;

#[derive(Debug, Clone)]
pub enum ConfirmIntent {
    QuitNoConnection,
    DeleteConnection(ConnectionId),
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
    title: String,
    message: String,
    intent: Option<ConfirmIntent>,
    pub preview_scroll: u16,
    pub preview_viewport_height: Option<u16>,
    pub preview_content_height: Option<u16>,
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
        self.preview_scroll = 0;
        self.preview_viewport_height = None;
        self.preview_content_height = None;
    }

    pub fn max_scroll(&self) -> u16 {
        match (self.preview_content_height, self.preview_viewport_height) {
            (Some(content), Some(viewport)) => content.saturating_sub(viewport),
            _ => 0,
        }
    }

    pub fn is_scrollable(&self) -> bool {
        self.max_scroll() > 0
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn intent(&self) -> Option<&ConfirmIntent> {
        self.intent.as_ref()
    }

    pub fn take_intent(&mut self) -> Option<ConfirmIntent> {
        self.intent.take()
    }
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: String::new(),
            intent: None,
            preview_scroll: 0,
            preview_viewport_height: None,
            preview_content_height: None,
        }
    }
}
