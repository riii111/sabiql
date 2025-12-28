use std::time::Instant;

use ratatui::widgets::ListState;
use tokio::sync::mpsc::Sender;

use super::action::Action;
use super::input_mode::InputMode;
use super::inspector_tab::InspectorTab;
use super::mode::Mode;
use super::result_history::ResultHistory;
use crate::domain::{DatabaseMetadata, MetadataState, QueryResult, Table, TableSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlModalState {
    #[default]
    Editing,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryState {
    #[default]
    Idle,
    Running,
}

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

    // Inspector sub-tabs
    pub inspector_tab: InspectorTab,

    // Result pane
    pub current_result: Option<QueryResult>,
    pub result_highlight_until: Option<Instant>,
    pub result_scroll_offset: usize,

    // Result history (for Adhoc queries)
    pub result_history: ResultHistory,
    pub history_index: Option<usize>,

    // SQL Modal
    pub sql_modal_content: String,
    pub sql_modal_cursor: usize,
    pub sql_modal_state: SqlModalState,

    // Query execution state
    pub query_state: QueryState,

    // Last error for copy functionality
    pub last_error: Option<String>,

    // Generation counter for race condition prevention
    pub selection_generation: u64,

    // Terminal dimensions for dynamic layout calculations
    pub terminal_height: u16,
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
            // Inspector sub-tabs
            inspector_tab: InspectorTab::default(),
            // Result pane
            current_result: None,
            result_highlight_until: None,
            result_scroll_offset: 0,
            // Result history
            result_history: ResultHistory::default(),
            history_index: None,
            // SQL Modal
            sql_modal_content: String::new(),
            sql_modal_cursor: 0,
            sql_modal_state: SqlModalState::default(),
            // Query state
            query_state: QueryState::default(),
            // Last error
            last_error: None,
            // Generation counter
            selection_generation: 0,
            // Terminal height (will be updated on resize)
            terminal_height: 24, // default minimum
        }
    }

    /// Calculate the number of visible rows in the result pane.
    /// Based on layout: header(1) + tabs(1) + main + footer(1) + cmdline(1) = 4 fixed
    /// Right side: 70% of main, Result: 50% of right side = 35% of main
    /// Result content = height - 2 (border) - 1 (header row) = height - 3
    pub fn result_visible_rows(&self) -> usize {
        let main_height = self.terminal_height.saturating_sub(4);
        // Result pane gets approximately 35% of main area (70% * 50%)
        let result_height = (main_height as u32 * 35 / 100) as u16;
        // Subtract borders (2) and header row (1)
        result_height.saturating_sub(3) as usize
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

    pub fn cache_age_display(&self) -> String {
        self.metadata
            .as_ref()
            .map(|m| format!("{}s", m.age_seconds()))
            .unwrap_or_else(|| "-".to_string())
    }
}
