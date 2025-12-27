use ratatui::widgets::ListState;

use super::action::Action;
use super::input_mode::InputMode;
use super::mode::Mode;
use crate::domain::{DatabaseMetadata, MetadataState, Table, TableSummary};
use tokio::sync::mpsc::Sender;

#[allow(dead_code)]
pub struct AppState {
    pub mode: Mode,
    pub should_quit: bool,
    pub project_name: String,
    pub profile_name: String,
    pub database_name: Option<String>,
    pub current_table: Option<String>,
    pub focus_mode: bool,
    pub active_tab: usize,
    pub input_mode: InputMode,
    pub command_line_input: String,
    pub filter_input: String,
    pub explorer_selected: usize,
    pub picker_selected: usize,

    pub explorer_list_state: ListState,
    pub picker_list_state: ListState,

    // Connection
    pub dsn: Option<String>,

    // Metadata
    pub metadata_state: MetadataState,
    pub metadata: Option<DatabaseMetadata>,

    // Selected table detail
    pub table_detail: Option<Table>,
    pub table_detail_state: MetadataState,

    // Action channel for async tasks
    pub action_tx: Option<Sender<Action>>,
}

impl AppState {
    pub fn new(project_name: String, profile_name: String) -> Self {
        Self {
            mode: Mode::default(),
            should_quit: false,
            project_name,
            profile_name,
            database_name: None,
            current_table: None,
            focus_mode: false,
            active_tab: 0,
            input_mode: InputMode::default(),
            command_line_input: String::new(),
            filter_input: String::new(),
            explorer_selected: 0,
            picker_selected: 0,
            explorer_list_state: ListState::default(),
            picker_list_state: ListState::default(),
            dsn: None,
            metadata_state: MetadataState::default(),
            metadata: None,
            table_detail: None,
            table_detail_state: MetadataState::default(),
            action_tx: None,
        }
    }

    pub fn tables(&self) -> Vec<&TableSummary> {
        self.metadata
            .as_ref()
            .map(|m| m.tables.iter().collect())
            .unwrap_or_default()
    }

    pub fn filtered_tables(&self) -> Vec<&TableSummary> {
        let filter_lower = self.filter_input.to_lowercase();
        self.tables()
            .into_iter()
            .filter(|t| t.qualified_name_lower().contains(&filter_lower))
            .collect()
    }

    pub fn send_action(&self, action: Action) {
        if let Some(tx) = &self.action_tx {
            let _ = tx.try_send(action);
        }
    }

    pub fn cache_age_display(&self) -> String {
        self.metadata
            .as_ref()
            .map(|m| format!("{}s", m.age_seconds()))
            .unwrap_or_else(|| "-".to_string())
    }
}
