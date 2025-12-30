use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use ratatui::widgets::ListState;
use tokio::sync::mpsc::Sender;

use super::action::Action;
use super::focused_pane::FocusedPane;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompletionKind {
    Keyword,
    Schema,
    Table,
    Column,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompletionCandidate {
    pub text: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub score: i32,
}

const RECENT_TABLES_MAX: usize = 10;
const RECENT_COLUMNS_MAX: usize = 20;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct CompletionState {
    pub visible: bool,
    pub candidates: Vec<CompletionCandidate>,
    pub selected_index: usize,
    pub trigger_position: usize,
    pub generation: u64,
    pub recent_tables: VecDeque<String>,
    pub recent_columns: VecDeque<String>,
}

impl CompletionState {
    /// Record a table as recently used
    #[allow(dead_code)]
    pub fn record_table(&mut self, table: String) {
        // Remove if already exists (to move to front)
        self.recent_tables.retain(|t| t != &table);
        self.recent_tables.push_front(table);
        if self.recent_tables.len() > RECENT_TABLES_MAX {
            self.recent_tables.pop_back();
        }
    }

    /// Record a column as recently used
    #[allow(dead_code)]
    pub fn record_column(&mut self, column: String) {
        // Remove if already exists (to move to front)
        self.recent_columns.retain(|c| c != &column);
        self.recent_columns.push_front(column);
        if self.recent_columns.len() > RECENT_COLUMNS_MAX {
            self.recent_columns.pop_back();
        }
    }

    /// Get recent columns as a Vec for completion scoring
    #[allow(dead_code)]
    pub fn recent_columns_vec(&self) -> Vec<String> {
        self.recent_columns.iter().cloned().collect()
    }
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
    pub focused_pane: FocusedPane,
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
    pub inspector_scroll_offset: usize,
    pub inspector_horizontal_offset: usize,
    pub inspector_max_horizontal_offset: usize,

    // Result pane
    pub current_result: Option<QueryResult>,
    pub result_highlight_until: Option<Instant>,
    pub result_scroll_offset: usize,
    pub result_horizontal_offset: usize,
    pub result_max_horizontal_offset: usize,

    // Result history (for Adhoc queries)
    pub result_history: ResultHistory,
    pub history_index: Option<usize>,

    // SQL Modal
    pub sql_modal_content: String,
    pub sql_modal_cursor: usize,
    pub sql_modal_state: SqlModalState,

    // SQL Modal completion
    pub completion: CompletionState,
    pub completion_debounce: Option<Instant>,

    // Tables currently being prefetched for completion (schema.table)
    pub prefetching_tables: HashSet<String>,

    // Tables that failed to prefetch (schema.table -> failure time) for backoff
    pub failed_prefetch_tables: HashMap<String, Instant>,

    // Prefetch queue for all tables (schema.table qualified names)
    pub prefetch_queue: VecDeque<String>,

    // Whether prefetch-all has been started for this SQL modal session
    pub prefetch_started: bool,

    // Query execution state
    pub query_state: QueryState,
    pub query_start_time: Option<Instant>,

    // Last error for copy functionality
    pub last_error: Option<String>,

    // Generation counter for race condition prevention
    pub selection_generation: u64,

    // Terminal dimensions for dynamic layout calculations
    pub terminal_height: u16,
    pub result_pane_height: u16,
    pub inspector_pane_height: u16,

    // Focus mode (Result full-screen)
    pub focus_mode: bool,
    pub focus_mode_prev_pane: Option<FocusedPane>,
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
            focused_pane: FocusedPane::default(),
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
            // Inspector
            inspector_tab: InspectorTab::default(),
            inspector_scroll_offset: 0,
            inspector_horizontal_offset: 0,
            inspector_max_horizontal_offset: 0,
            // Result pane
            current_result: None,
            result_highlight_until: None,
            result_scroll_offset: 0,
            result_horizontal_offset: 0,
            result_max_horizontal_offset: 0,
            // Result history
            result_history: ResultHistory::default(),
            history_index: None,
            // SQL Modal
            sql_modal_content: String::new(),
            sql_modal_cursor: 0,
            sql_modal_state: SqlModalState::default(),
            completion: CompletionState::default(),
            completion_debounce: None,
            prefetching_tables: HashSet::new(),
            failed_prefetch_tables: HashMap::new(),
            prefetch_queue: VecDeque::new(),
            prefetch_started: false,
            // Query state
            query_state: QueryState::default(),
            query_start_time: None,
            // Last error
            last_error: None,
            // Generation counter
            selection_generation: 0,
            // Terminal height (will be updated on resize)
            terminal_height: 24,      // default minimum
            result_pane_height: 0,    // will be updated on render
            inspector_pane_height: 0, // will be updated on render
            // Focus mode
            focus_mode: false,
            focus_mode_prev_pane: None,
        }
    }

    /// Calculate the number of visible rows in the result pane.
    /// Uses the actual result pane height from the last render.
    /// Result content = height - 2 (border) - 1 (header row) = height - 3
    pub fn result_visible_rows(&self) -> usize {
        self.result_pane_height.saturating_sub(3) as usize
    }

    /// Calculate the number of visible rows in the inspector pane.
    /// Inspector content = height - 2 (border) - 1 (header row) - 1 (scroll indicator) = height - 4
    pub fn inspector_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(4) as usize
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

    pub fn change_tab(&mut self, next: bool) {
        const TAB_COUNT: usize = 2;
        self.active_tab = if next {
            (self.active_tab + 1) % TAB_COUNT
        } else {
            (self.active_tab + TAB_COUNT - 1) % TAB_COUNT
        };
        self.mode = Mode::from_tab_index(self.active_tab);
        self.focused_pane = self.mode.default_pane();
        self.focus_mode = false;
        self.focus_mode_prev_pane = None;
        self.result_scroll_offset = 0;
        self.result_horizontal_offset = 0;
    }

    #[allow(dead_code)]
    pub fn can_enter_focus(&self) -> bool {
        self.mode == Mode::Browse && !self.focus_mode
    }

    pub fn toggle_focus(&mut self) -> bool {
        if self.mode != Mode::Browse {
            return false;
        }
        if self.focus_mode {
            if let Some(prev) = self.focus_mode_prev_pane.take() {
                self.focused_pane = prev;
            }
            self.focus_mode = false;
        } else {
            self.focus_mode_prev_pane = Some(self.focused_pane);
            self.focused_pane = FocusedPane::Result;
            self.focus_mode = true;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn default_result_pane_height_returns_zero_visible_rows() {
        let state = AppState::new("test".to_string(), "default".to_string());

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[rstest]
    #[case(10, 7)]
    #[case(15, 12)]
    #[case(20, 17)]
    #[case(30, 27)]
    fn result_pane_height_calculates_correct_visible_rows(
        #[case] pane_height: u16,
        #[case] expected: usize,
    ) {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.result_pane_height = pane_height;

        let visible = state.result_visible_rows();

        assert_eq!(visible, expected);
    }

    #[test]
    fn small_result_pane_height_does_not_underflow() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.result_pane_height = 2;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[test]
    fn very_small_result_pane_returns_zero_rows() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.result_pane_height = 1;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[test]
    fn large_result_pane_height_returns_proportional_rows() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.result_pane_height = 50;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 47);
    }

    #[test]
    fn filtered_tables_with_empty_filter_returns_all() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: std::time::Instant::now(),
        });
        state.filter_input = "".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filtered_tables_with_matching_filter_returns_subset() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: std::time::Instant::now(),
        });
        state.filter_input = "user".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "users");
    }

    #[test]
    fn filtered_tables_is_case_insensitive() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![TableSummary::new(
                "public".to_string(),
                "Users".to_string(),
                Some(100),
                false,
            )],
            fetched_at: std::time::Instant::now(),
        });
        state.filter_input = "user".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn selection_generation_starts_at_zero() {
        let state = AppState::new("test".to_string(), "default".to_string());

        assert_eq!(state.selection_generation, 0);
    }

    #[test]
    fn selection_generation_increments_prevent_race_conditions() {
        let mut state = AppState::new("test".to_string(), "default".to_string());

        let gen1 = state.selection_generation;
        state.selection_generation += 1;
        let gen2 = state.selection_generation;
        state.selection_generation += 1;
        let gen3 = state.selection_generation;

        assert_eq!(gen1, 0);
        assert_eq!(gen2, 1);
        assert_eq!(gen3, 2);
    }

    #[test]
    fn selection_generation_can_detect_stale_responses() {
        let mut state = AppState::new("test".to_string(), "default".to_string());

        let initial_gen = state.selection_generation;
        state.selection_generation += 1;
        let current_gen = state.selection_generation;

        assert!(initial_gen < current_gen);
    }

    // Tab change tests

    #[test]
    fn change_tab_next_updates_mode_and_pane() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        assert_eq!(state.mode, Mode::Browse);

        state.change_tab(true);

        assert_eq!(state.mode, Mode::ER);
        assert_eq!(state.focused_pane, Mode::ER.default_pane());
    }

    #[test]
    fn change_tab_prev_updates_mode_and_pane() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.active_tab = 1;
        state.mode = Mode::ER;

        state.change_tab(false);

        assert_eq!(state.mode, Mode::Browse);
        assert_eq!(state.focused_pane, Mode::Browse.default_pane());
    }

    #[test]
    fn change_tab_resets_focus_mode() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.focus_mode = true;
        state.focus_mode_prev_pane = Some(FocusedPane::Explorer);

        state.change_tab(true);

        assert!(!state.focus_mode);
    }

    #[test]
    fn change_tab_while_in_focus_mode_exits_safely() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.toggle_focus();
        assert!(state.focus_mode);

        state.change_tab(true);

        assert!(!state.focus_mode);
        assert_eq!(state.mode, Mode::ER);
    }

    // Focus mode tests

    #[test]
    fn toggle_focus_enters_focus_mode_in_browse() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.focused_pane = FocusedPane::Explorer;

        let result = state.toggle_focus();

        assert!(result);
        assert!(state.focus_mode);
        assert_eq!(state.focused_pane, FocusedPane::Result);
        assert_eq!(state.focus_mode_prev_pane, Some(FocusedPane::Explorer));
    }

    #[test]
    fn toggle_focus_exits_focus_mode_and_restores_pane() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.focused_pane = FocusedPane::Inspector;
        state.toggle_focus();

        let result = state.toggle_focus();

        assert!(result);
        assert!(!state.focus_mode);
        assert_eq!(state.focused_pane, FocusedPane::Inspector);
    }

    #[test]
    fn toggle_focus_is_blocked_in_er_mode() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.mode = Mode::ER;

        let result = state.toggle_focus();

        assert!(!result);
        assert!(!state.focus_mode);
    }

    #[test]
    fn can_enter_focus_true_in_browse_mode() {
        let state = AppState::new("test".to_string(), "default".to_string());

        assert!(state.can_enter_focus());
    }

    #[test]
    fn can_enter_focus_false_in_er_mode() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.mode = Mode::ER;

        assert!(!state.can_enter_focus());
    }

    #[test]
    fn can_enter_focus_false_when_already_in_focus() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.toggle_focus();

        assert!(!state.can_enter_focus());
    }

    // CompletionState recent tracking tests

    #[test]
    fn record_table_adds_to_recent() {
        let mut cs = CompletionState::default();

        cs.record_table("users".to_string());

        assert_eq!(cs.recent_tables.len(), 1);
        assert_eq!(cs.recent_tables[0], "users");
    }

    #[test]
    fn record_table_moves_existing_to_front() {
        let mut cs = CompletionState::default();
        cs.record_table("users".to_string());
        cs.record_table("orders".to_string());

        cs.record_table("users".to_string());

        assert_eq!(cs.recent_tables.len(), 2);
        assert_eq!(cs.recent_tables[0], "users");
        assert_eq!(cs.recent_tables[1], "orders");
    }

    #[test]
    fn record_table_limits_to_max_size() {
        let mut cs = CompletionState::default();

        for i in 0..15 {
            cs.record_table(format!("table_{}", i));
        }

        assert_eq!(cs.recent_tables.len(), RECENT_TABLES_MAX);
        assert_eq!(cs.recent_tables[0], "table_14");
    }

    #[test]
    fn record_column_adds_to_recent() {
        let mut cs = CompletionState::default();

        cs.record_column("id".to_string());

        assert_eq!(cs.recent_columns.len(), 1);
        assert_eq!(cs.recent_columns[0], "id");
    }

    #[test]
    fn record_column_moves_existing_to_front() {
        let mut cs = CompletionState::default();
        cs.record_column("id".to_string());
        cs.record_column("name".to_string());

        cs.record_column("id".to_string());

        assert_eq!(cs.recent_columns.len(), 2);
        assert_eq!(cs.recent_columns[0], "id");
        assert_eq!(cs.recent_columns[1], "name");
    }

    #[test]
    fn record_column_limits_to_max_size() {
        let mut cs = CompletionState::default();

        for i in 0..25 {
            cs.record_column(format!("col_{}", i));
        }

        assert_eq!(cs.recent_columns.len(), RECENT_COLUMNS_MAX);
        assert_eq!(cs.recent_columns[0], "col_24");
    }

    #[test]
    fn recent_columns_vec_returns_vec() {
        let mut cs = CompletionState::default();
        cs.record_column("id".to_string());
        cs.record_column("name".to_string());

        let vec = cs.recent_columns_vec();

        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0], "name");
        assert_eq!(vec[1], "id");
    }
}
