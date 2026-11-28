use std::sync::Arc;

use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{ConnectionId, QueryResult};
use crate::model::shared::scroll::ScrollOffset;
use crate::update::action::ScrollDirection;

#[derive(Debug, Clone)]
pub enum ConfirmIntent {
    QuitNoConnection,
    DeleteConnection(ConnectionId),
    ExecuteWrite {
        sql: String,
        blocked: bool,
    },
    CsvExportCached {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        file_name: String,
        result: Arc<QueryResult>,
    },
    DisableReadOnly,
}

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) intent: Option<ConfirmIntent>,
    pub(crate) preview_scroll: ScrollOffset,
    pub(crate) preview_viewport_height: Option<u16>,
    pub(crate) preview_content_height: Option<u16>,
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
        self.preview_scroll.set(0);
        self.preview_viewport_height = None;
        self.preview_content_height = None;
    }

    pub fn max_scroll(&self) -> u16 {
        match (self.preview_content_height, self.preview_viewport_height) {
            (Some(content), Some(viewport)) => content.saturating_sub(viewport),
            _ => 0,
        }
    }

    pub fn preview_scroll(&self) -> u16 {
        self.preview_scroll.get() as u16
    }

    pub fn apply_preview_metrics(
        &mut self,
        viewport_height: Option<u16>,
        content_height: Option<u16>,
        scroll: u16,
    ) {
        self.preview_viewport_height = viewport_height;
        self.preview_content_height = content_height;
        self.preview_scroll
            .set(scroll.min(self.max_scroll()) as usize);
    }

    pub fn scroll_preview(&mut self, direction: ScrollDirection) {
        let max_scroll = self.max_scroll() as usize;
        self.preview_scroll.scroll(direction, 1, max_scroll);
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
            preview_scroll: ScrollOffset::default(),
            preview_viewport_height: None,
            preview_content_height: None,
        }
    }
}
