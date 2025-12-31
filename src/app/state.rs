use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use ratatui::widgets::ListState;
use tokio::sync::mpsc::Sender;

use super::action::Action;
use super::focused_pane::FocusedPane;
use super::input_mode::InputMode;
use super::inspector_tab::InspectorTab;
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
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub text: String,
    pub kind: CompletionKind,
    pub score: i32,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    pub visible: bool,
    pub candidates: Vec<CompletionCandidate>,
    pub selected_index: usize,
    pub trigger_position: usize,
    pub recent_columns: VecDeque<String>,
}

impl CompletionState {
    /// Get recent columns as a Vec for completion scoring
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErStatus {
    #[default]
    Idle,
    Waiting,   // User pressed 'e' but prefetch not complete
    Rendering, // DOT generation in progress
}

pub struct AppState {
    pub should_quit: bool,
    pub project_name: String,
    pub profile_name: String,
    pub database_name: Option<String>,
    pub current_table: Option<String>,
    pub focused_pane: FocusedPane,
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

    // Tables that failed to prefetch (schema.table -> (failure time, error message))
    pub failed_prefetch_tables: HashMap<String, (Instant, String)>,

    // Prefetch queue for all tables (schema.table qualified names)
    pub prefetch_queue: VecDeque<String>,

    // Whether prefetch-all has been started for this SQL modal session
    pub prefetch_started: bool,

    // Query execution state
    pub query_state: QueryState,
    pub query_start_time: Option<Instant>,

    // Status messages (shown in footer, auto-clear after timeout)
    pub last_error: Option<String>,
    pub last_success: Option<String>,
    pub message_expires_at: Option<Instant>,

    // Generation counter for race condition prevention
    pub selection_generation: u64,

    // Terminal dimensions for dynamic layout calculations
    pub terminal_height: u16,
    pub result_pane_height: u16,
    pub inspector_pane_height: u16,

    // Focus mode (Result full-screen)
    pub focus_mode: bool,
    pub focus_mode_prev_pane: Option<FocusedPane>,

    // ER diagram status
    pub er_status: ErStatus,
}

impl AppState {
    pub fn new(project_name: String, profile_name: String) -> Self {
        Self {
            should_quit: false,
            project_name,
            profile_name,
            database_name: None,
            current_table: None,
            focused_pane: FocusedPane::default(),
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
            // Status messages
            last_error: None,
            last_success: None,
            message_expires_at: None,
            // Generation counter
            selection_generation: 0,
            // Terminal height (will be updated on resize)
            terminal_height: 24,      // default minimum
            result_pane_height: 0,    // will be updated on render
            inspector_pane_height: 0, // will be updated on render
            // Focus mode
            focus_mode: false,
            focus_mode_prev_pane: None,
            // ER diagram status
            er_status: ErStatus::default(),
        }
    }

    const MESSAGE_TIMEOUT_SECS: u64 = 3;

    pub fn set_error(&mut self, msg: String) {
        self.last_error = Some(msg);
        self.last_success = None;
        self.message_expires_at =
            Some(Instant::now() + std::time::Duration::from_secs(Self::MESSAGE_TIMEOUT_SECS));
    }

    pub fn set_success(&mut self, msg: String) {
        self.last_success = Some(msg);
        self.last_error = None;
        self.message_expires_at =
            Some(Instant::now() + std::time::Duration::from_secs(Self::MESSAGE_TIMEOUT_SECS));
    }

    pub fn clear_expired_messages(&mut self) {
        if let Some(expires) = self.message_expires_at
            && expires <= Instant::now()
        {
            self.last_error = None;
            self.last_success = None;
            self.message_expires_at = None;
        }
    }

    /// Calculate the number of visible rows in the result pane.
    /// Uses the actual result pane height from the last render.
    /// Result content = height - 2 (border) - 1 (header row) = height - 3
    pub fn result_visible_rows(&self) -> usize {
        self.result_pane_height.saturating_sub(3) as usize
    }

    /// Calculate the number of visible rows in the inspector pane.
    /// Inspector content = height - 2 (border) - 1 (tab bar) - 1 (header row) - 1 (scroll indicator) = height - 5
    pub fn inspector_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(5) as usize
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

    pub fn toggle_focus(&mut self) -> bool {
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

    // Focus mode tests

    #[test]
    fn toggle_focus_enters_focus_mode() {
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

    // Prefetch state tests

    #[test]
    fn prefetch_queue_starts_empty() {
        let state = AppState::new("test".to_string(), "default".to_string());

        assert!(state.prefetch_queue.is_empty());
        assert!(!state.prefetch_started);
    }

    #[test]
    fn prefetch_queue_pop_returns_fifo_order() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        state.prefetch_queue.push_back("public.users".to_string());
        state.prefetch_queue.push_back("public.orders".to_string());

        let first = state.prefetch_queue.pop_front();
        let second = state.prefetch_queue.pop_front();

        assert_eq!(first, Some("public.users".to_string()));
        assert_eq!(second, Some("public.orders".to_string()));
    }

    #[test]
    fn prefetching_tables_tracks_in_flight() {
        let mut state = AppState::new("test".to_string(), "default".to_string());

        state.prefetching_tables.insert("public.users".to_string());

        assert!(state.prefetching_tables.contains("public.users"));
        assert!(!state.prefetching_tables.contains("public.orders"));
    }

    #[test]
    fn failed_prefetch_tables_tracks_failure_time_and_error() {
        let mut state = AppState::new("test".to_string(), "default".to_string());
        let now = Instant::now();

        state.failed_prefetch_tables.insert(
            "public.users".to_string(),
            (now, "connection timeout".to_string()),
        );

        assert!(state.failed_prefetch_tables.contains_key("public.users"));
        let (instant, error) = state.failed_prefetch_tables.get("public.users").unwrap();
        assert!(instant.elapsed().as_secs() < 1);
        assert_eq!(error, "connection timeout");
    }

    mod er_status {
        use super::*;

        #[test]
        fn new_state_defaults_to_idle() {
            let state = AppState::new("test".to_string(), "default".to_string());

            assert_eq!(state.er_status, ErStatus::Idle);
        }

        #[test]
        fn idle_with_incomplete_prefetch_transitions_to_waiting() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.prefetch_queue.push_back("public.users".to_string());

            let prefetch_complete =
                state.prefetch_queue.is_empty() && state.prefetching_tables.is_empty();
            if !prefetch_complete {
                state.er_status = ErStatus::Waiting;
            }

            assert_eq!(state.er_status, ErStatus::Waiting);
        }

        #[test]
        fn idle_with_complete_prefetch_transitions_to_rendering() {
            let mut state = AppState::new("test".to_string(), "default".to_string());

            let prefetch_complete =
                state.prefetch_queue.is_empty() && state.prefetching_tables.is_empty();
            if prefetch_complete {
                state.er_status = ErStatus::Rendering;
            }

            assert_eq!(state.er_status, ErStatus::Rendering);
        }

        #[test]
        fn waiting_with_prefetch_complete_transitions_to_idle() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.er_status = ErStatus::Waiting;
            state.prefetch_queue.push_back("public.users".to_string());

            state.prefetch_queue.pop_front();
            let prefetch_complete =
                state.prefetch_queue.is_empty() && state.prefetching_tables.is_empty();
            if state.er_status == ErStatus::Waiting && prefetch_complete {
                state.er_status = ErStatus::Idle;
            }

            assert_eq!(state.er_status, ErStatus::Idle);
        }

        #[test]
        fn rendering_on_diagram_opened_transitions_to_idle() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.er_status = ErStatus::Rendering;

            state.er_status = ErStatus::Idle;

            assert_eq!(state.er_status, ErStatus::Idle);
        }

        #[test]
        fn rendering_on_diagram_failed_transitions_to_idle() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.er_status = ErStatus::Rendering;

            state.er_status = ErStatus::Idle;

            assert_eq!(state.er_status, ErStatus::Idle);
        }

        #[test]
        fn waiting_with_in_flight_tables_remains_waiting() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.er_status = ErStatus::Waiting;
            state.prefetching_tables.insert("public.orders".to_string());

            let prefetch_complete =
                state.prefetch_queue.is_empty() && state.prefetching_tables.is_empty();

            assert!(!prefetch_complete);
            assert_eq!(state.er_status, ErStatus::Waiting);
        }
    }

    mod reload_metadata_reset {
        use super::*;

        #[test]
        fn clears_prefetch_state() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.prefetch_started = true;
            state.prefetch_queue.push_back("public.users".to_string());
            state.prefetching_tables.insert("public.orders".to_string());
            state.failed_prefetch_tables.insert(
                "public.failed".to_string(),
                (Instant::now(), "timeout".to_string()),
            );

            // Simulate ReloadMetadata reset
            state.prefetch_started = false;
            state.prefetch_queue.clear();
            state.prefetching_tables.clear();
            state.failed_prefetch_tables.clear();

            assert!(!state.prefetch_started);
            assert!(state.prefetch_queue.is_empty());
            assert!(state.prefetching_tables.is_empty());
            assert!(state.failed_prefetch_tables.is_empty());
        }

        #[test]
        fn resets_er_status_to_idle() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.er_status = ErStatus::Waiting;

            // Simulate ReloadMetadata reset
            state.er_status = ErStatus::Idle;

            assert_eq!(state.er_status, ErStatus::Idle);
        }

        #[test]
        fn clears_stale_messages() {
            let mut state = AppState::new("test".to_string(), "default".to_string());
            state.set_error("Old error".to_string());

            // Simulate ReloadMetadata reset
            state.last_error = None;
            state.last_success = None;
            state.message_expires_at = None;

            assert!(state.last_error.is_none());
            assert!(state.last_success.is_none());
            assert!(state.message_expires_at.is_none());
        }
    }
}
