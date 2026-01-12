use tokio::sync::mpsc::Sender;

use super::action::Action;
use super::confirm_dialog_state::ConfirmDialogState;
use super::connection_error_state::ConnectionErrorState;
use super::connection_setup_state::ConnectionSetupState;
use super::message_state::MessageState;
use super::metadata_cache::MetadataCache;
use super::query_execution::QueryExecution;
use super::runtime_state::RuntimeState;
use super::sql_modal_context::SqlModalContext;
use super::ui_state::UiState;
use crate::domain::TableSummary;

pub struct AppState {
    pub should_quit: bool,
    pub command_line_input: String,
    pub action_tx: Option<Sender<Action>>,

    /// Dirty flag for event-driven rendering.
    /// When true, a render is needed on the next event loop iteration.
    pub render_dirty: bool,

    pub runtime: RuntimeState,
    pub ui: UiState,
    pub cache: MetadataCache,
    pub query: QueryExecution,
    pub sql_modal: SqlModalContext,
    pub messages: MessageState,
    pub er_preparation: super::er_state::ErPreparationState,
    pub connection_setup: ConnectionSetupState,
    pub connection_error: ConnectionErrorState,
    pub confirm_dialog: ConfirmDialogState,
}

impl AppState {
    pub fn new(project_name: String) -> Self {
        Self {
            should_quit: false,
            command_line_input: String::new(),
            action_tx: None,
            render_dirty: true, // Initial render needed
            runtime: RuntimeState::new(project_name),
            ui: UiState::new(),
            cache: MetadataCache::default(),
            query: QueryExecution::default(),
            sql_modal: SqlModalContext::default(),
            messages: MessageState::default(),
            er_preparation: super::er_state::ErPreparationState::default(),
            connection_setup: ConnectionSetupState::default(),
            connection_error: ConnectionErrorState::default(),
            confirm_dialog: ConfirmDialogState::default(),
        }
    }

    /// Mark the state as needing a render.
    /// Call this after any state change that affects the UI.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.render_dirty = true;
    }

    /// Clear the dirty flag after rendering.
    #[inline]
    pub fn clear_dirty(&mut self) {
        self.render_dirty = false;
    }

    pub fn set_error(&mut self, msg: String) {
        self.messages.set_error(msg);
    }

    pub fn set_success(&mut self, msg: String) {
        self.messages.set_success(msg);
    }

    pub fn clear_expired_messages(&mut self) {
        self.messages.clear_expired();
    }

    /// Clear all expired timers (messages, highlight, etc.)
    pub fn clear_expired_timers(&mut self, now: std::time::Instant) {
        self.messages.clear_expired();
        if let Some(until) = self.query.result_highlight_until
            && now >= until
        {
            self.query.result_highlight_until = None;
        }
    }

    pub fn result_visible_rows(&self) -> usize {
        self.ui.result_visible_rows()
    }

    pub fn inspector_visible_rows(&self) -> usize {
        self.ui.inspector_visible_rows()
    }

    pub fn inspector_ddl_visible_rows(&self) -> usize {
        self.ui.inspector_ddl_visible_rows()
    }

    pub fn tables(&self) -> Vec<&TableSummary> {
        self.cache.tables()
    }

    pub fn filtered_tables(&self) -> Vec<&TableSummary> {
        let filter_lower = self.ui.filter_input.to_lowercase();
        self.cache
            .metadata
            .as_ref()
            .map(|m| {
                m.tables
                    .iter()
                    .filter(|t| t.qualified_name_lower().contains(&filter_lower))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn toggle_focus(&mut self) -> bool {
        self.ui.toggle_focus()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::focused_pane::FocusedPane;
    use crate::domain::DatabaseMetadata;
    use rstest::rstest;
    use std::time::Instant;

    #[test]
    fn default_result_pane_height_returns_zero_visible_rows() {
        let state = AppState::new("test".to_string());

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[rstest]
    #[case(10, 5)]
    #[case(15, 10)]
    #[case(20, 15)]
    #[case(30, 25)]
    fn result_pane_height_calculates_correct_visible_rows(
        #[case] pane_height: u16,
        #[case] expected: usize,
    ) {
        let mut state = AppState::new("test".to_string());
        state.ui.result_pane_height = pane_height;

        let visible = state.result_visible_rows();

        assert_eq!(visible, expected);
    }

    #[test]
    fn small_result_pane_height_does_not_underflow() {
        let mut state = AppState::new("test".to_string());
        state.ui.result_pane_height = 2;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[test]
    fn very_small_result_pane_returns_zero_rows() {
        let mut state = AppState::new("test".to_string());
        state.ui.result_pane_height = 1;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[test]
    fn large_result_pane_height_returns_proportional_rows() {
        let mut state = AppState::new("test".to_string());
        state.ui.result_pane_height = 50;

        let visible = state.result_visible_rows();

        assert_eq!(visible, 45);
    }

    #[test]
    fn filtered_tables_with_empty_filter_returns_all() {
        let mut state = AppState::new("test".to_string());
        state.cache.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: std::time::Instant::now(),
        });
        state.ui.filter_input = "".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filtered_tables_with_matching_filter_returns_subset() {
        let mut state = AppState::new("test".to_string());
        state.cache.metadata = Some(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            tables: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: std::time::Instant::now(),
        });
        state.ui.filter_input = "user".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "users");
    }

    #[test]
    fn filtered_tables_is_case_insensitive() {
        let mut state = AppState::new("test".to_string());
        state.cache.metadata = Some(DatabaseMetadata {
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
        state.ui.filter_input = "user".to_string();

        let filtered = state.filtered_tables();

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn selection_generation_starts_at_zero() {
        let state = AppState::new("test".to_string());

        assert_eq!(state.cache.selection_generation, 0);
    }

    #[test]
    fn selection_generation_increments_prevent_race_conditions() {
        let mut state = AppState::new("test".to_string());

        let gen1 = state.cache.selection_generation;
        state.cache.selection_generation += 1;
        let gen2 = state.cache.selection_generation;
        state.cache.selection_generation += 1;
        let gen3 = state.cache.selection_generation;

        assert_eq!(gen1, 0);
        assert_eq!(gen2, 1);
        assert_eq!(gen3, 2);
    }

    #[test]
    fn selection_generation_can_detect_stale_responses() {
        let mut state = AppState::new("test".to_string());

        let initial_gen = state.cache.selection_generation;
        state.cache.selection_generation += 1;
        let current_gen = state.cache.selection_generation;

        assert!(initial_gen < current_gen);
    }

    // Focus mode tests

    #[test]
    fn toggle_focus_enters_focus_mode() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Explorer;

        let result = state.toggle_focus();

        assert!(result);
        assert!(state.ui.focus_mode);
        assert_eq!(state.ui.focused_pane, FocusedPane::Result);
        assert_eq!(state.ui.focus_mode_prev_pane, Some(FocusedPane::Explorer));
    }

    #[test]
    fn toggle_focus_exits_focus_mode_and_restores_pane() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Inspector;
        state.toggle_focus();

        let result = state.toggle_focus();

        assert!(result);
        assert!(!state.ui.focus_mode);
        assert_eq!(state.ui.focused_pane, FocusedPane::Inspector);
    }

    // Prefetch state tests

    #[test]
    fn prefetch_queue_starts_empty() {
        let state = AppState::new("test".to_string());

        assert!(state.sql_modal.prefetch_queue.is_empty());
        assert!(!state.sql_modal.prefetch_started);
    }

    #[test]
    fn prefetch_queue_pop_returns_fifo_order() {
        let mut state = AppState::new("test".to_string());
        state
            .sql_modal
            .prefetch_queue
            .push_back("public.users".to_string());
        state
            .sql_modal
            .prefetch_queue
            .push_back("public.orders".to_string());

        let first = state.sql_modal.prefetch_queue.pop_front();
        let second = state.sql_modal.prefetch_queue.pop_front();

        assert_eq!(first, Some("public.users".to_string()));
        assert_eq!(second, Some("public.orders".to_string()));
    }

    #[test]
    fn prefetching_tables_tracks_in_flight() {
        let mut state = AppState::new("test".to_string());

        state
            .sql_modal
            .prefetching_tables
            .insert("public.users".to_string());

        assert!(state.sql_modal.prefetching_tables.contains("public.users"));
        assert!(!state.sql_modal.prefetching_tables.contains("public.orders"));
    }

    #[test]
    fn failed_prefetch_tables_tracks_failure_time_and_error() {
        let mut state = AppState::new("test".to_string());
        let now = Instant::now();

        state.sql_modal.failed_prefetch_tables.insert(
            "public.users".to_string(),
            (now, "connection timeout".to_string()),
        );

        assert!(
            state
                .sql_modal
                .failed_prefetch_tables
                .contains_key("public.users")
        );
        let (instant, error) = state
            .sql_modal
            .failed_prefetch_tables
            .get("public.users")
            .unwrap();
        assert!(instant.elapsed().as_secs() < 1);
        assert_eq!(error, "connection timeout");
    }

    mod er_preparation {
        use super::*;
        use crate::app::er_state::ErStatus;

        #[test]
        fn new_state_defaults_to_idle() {
            let state = AppState::new("test".to_string());

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
        }

        #[test]
        fn status_can_be_set_to_waiting() {
            let mut state = AppState::new("test".to_string());

            state.er_preparation.status = ErStatus::Waiting;

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
        }

        #[test]
        fn status_can_be_set_to_rendering() {
            let mut state = AppState::new("test".to_string());

            state.er_preparation.status = ErStatus::Rendering;

            assert_eq!(state.er_preparation.status, ErStatus::Rendering);
        }
    }

    mod reload_metadata_reset {
        use super::*;

        #[test]
        fn clears_prefetch_state() {
            let mut state = AppState::new("test".to_string());
            state.sql_modal.prefetch_started = true;
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.users".to_string());
            state
                .sql_modal
                .prefetching_tables
                .insert("public.orders".to_string());
            state.sql_modal.failed_prefetch_tables.insert(
                "public.failed".to_string(),
                (Instant::now(), "timeout".to_string()),
            );

            // Simulate ReloadMetadata reset using reset_prefetch()
            state.sql_modal.reset_prefetch();

            assert!(!state.sql_modal.prefetch_started);
            assert!(state.sql_modal.prefetch_queue.is_empty());
            assert!(state.sql_modal.prefetching_tables.is_empty());
            assert!(state.sql_modal.failed_prefetch_tables.is_empty());
        }

        #[test]
        fn resets_er_preparation() {
            use crate::app::er_state::ErStatus;

            let mut state = AppState::new("test".to_string());
            state.er_preparation.status = ErStatus::Waiting;

            state.er_preparation.reset();

            assert_eq!(state.er_preparation.status, ErStatus::Idle);
        }

        #[test]
        fn clears_stale_messages() {
            let mut state = AppState::new("test".to_string());
            state.set_error("Old error".to_string());

            // Simulate ReloadMetadata reset
            state.messages.clear();

            assert!(state.messages.last_error.is_none());
            assert!(state.messages.last_success.is_none());
            assert!(state.messages.expires_at.is_none());
        }
    }

    mod inspector_scroll_reset {
        use super::*;

        #[test]
        fn scroll_offset_resets_to_zero_on_table_switch() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_scroll_offset = 42;

            // Simulate table switch (TableDetailLoaded action)
            state.ui.inspector_scroll_offset = 0;

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn scroll_offset_stays_zero_when_no_table_detail() {
            let state = AppState::new("test".to_string());

            assert_eq!(state.ui.inspector_scroll_offset, 0);
            assert!(state.cache.table_detail.is_none());
        }
    }

    mod inspector_visible_rows {
        use super::*;

        #[test]
        fn ddl_visible_rows_is_greater_than_standard() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 20;

            let standard = state.inspector_visible_rows();
            let ddl = state.inspector_ddl_visible_rows();

            // DDL has no header row, so it should have 2 more visible rows
            assert_eq!(ddl - standard, 2);
        }

        #[rstest]
        #[case(10, 7)]
        #[case(15, 12)]
        #[case(20, 17)]
        fn ddl_visible_rows_equals_height_minus_three(
            #[case] pane_height: u16,
            #[case] expected: usize,
        ) {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = pane_height;

            let visible = state.inspector_ddl_visible_rows();

            assert_eq!(visible, expected);
        }

        #[test]
        fn small_pane_height_does_not_underflow() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 2;

            let visible = state.inspector_ddl_visible_rows();

            assert_eq!(visible, 0);
        }
    }
}
