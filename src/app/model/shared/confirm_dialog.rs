use std::fmt;

use crate::domain::{ConnectionId, QueryValue};
use crate::policy::mask_password;
use crate::update::action::ScrollDirection;

#[derive(Debug, Clone)]
pub struct CsvExportCacheSnapshot {
    pub columns: Vec<String>,
    pub values: Vec<Vec<QueryValue>>,
}

#[derive(Clone)]
pub enum ConfirmIntent {
    QuitNoConnection,
    DeleteConnection(ConnectionId),
    ExecuteWrite {
        sql: String,
        blocked: bool,
    },
    CsvExportRerunnable {
        dsn: String,
        run_id: u64,
        export_query: String,
        file_name: String,
    },
    CsvExportCached {
        dsn: String,
        run_id: u64,
        file_name: String,
        row_count: Option<usize>,
        snapshot: CsvExportCacheSnapshot,
    },
    DisableReadOnly,
}

struct MaskedDsn<'a>(&'a str);

impl fmt::Debug for MaskedDsn<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&mask_password(self.0))
    }
}

impl fmt::Debug for ConfirmIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CsvExportRerunnable {
                dsn,
                run_id,
                export_query,
                file_name,
            } => formatter
                .debug_struct("ConfirmIntent::CsvExportRerunnable")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("export_query", export_query)
                .field("file_name", file_name)
                .finish(),
            Self::CsvExportCached {
                dsn,
                run_id,
                file_name,
                row_count,
                snapshot,
            } => formatter
                .debug_struct("ConfirmIntent::CsvExportCached")
                .field("dsn", &MaskedDsn(dsn))
                .field("run_id", run_id)
                .field("file_name", file_name)
                .field("row_count", row_count)
                .field("snapshot", snapshot)
                .finish(),
            Self::QuitNoConnection => formatter.write_str("ConfirmIntent::QuitNoConnection"),
            Self::DeleteConnection(id) => formatter
                .debug_tuple("ConfirmIntent::DeleteConnection")
                .field(id)
                .finish(),
            Self::ExecuteWrite { sql, blocked } => formatter
                .debug_struct("ConfirmIntent::ExecuteWrite")
                .field("sql", sql)
                .field("blocked", blocked)
                .finish(),
            Self::DisableReadOnly => formatter.write_str("ConfirmIntent::DisableReadOnly"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) intent: Option<ConfirmIntent>,
    pub(crate) preview_scroll: u16,
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

    pub fn preview_scroll(&self) -> u16 {
        self.preview_scroll
    }

    pub fn apply_preview_metrics(
        &mut self,
        viewport_height: Option<u16>,
        content_height: Option<u16>,
        scroll: u16,
    ) {
        self.preview_viewport_height = viewport_height;
        self.preview_content_height = content_height;
        self.preview_scroll = scroll.min(self.max_scroll());
    }

    pub fn scroll_preview(&mut self, direction: ScrollDirection) {
        let max_scroll = self.max_scroll() as usize;
        self.preview_scroll =
            direction.clamp_vertical_offset(self.preview_scroll as usize, max_scroll, 1) as u16;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_uri_passwords_in_confirm_state() {
        let intent = ConfirmIntent::CsvExportRerunnable {
            dsn: "postgresql://user@host/db?pass%77ord=secret".to_string(),
            run_id: 1,
            export_query: "SELECT 1".to_string(),
            file_name: "export.csv".to_string(),
        };
        let state = ConfirmDialogState {
            intent: Some(intent),
            ..Default::default()
        };
        let debug = format!("{state:?}");

        assert!(!debug.contains("secret"));
        assert!(debug.contains("pass%77ord=****"));
    }
}
