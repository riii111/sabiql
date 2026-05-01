use std::time::Instant;

use super::explain_context::ExplainContext;
use super::runtime_state::RuntimeState;
use crate::domain::TableSummary;
use crate::domain::connection::{ConnectionProfile, ServiceEntry};
use crate::model::browse::jsonb_detail::JsonbDetailState;
use crate::model::browse::query_execution::QueryExecution;
use crate::model::browse::result_interaction::ResultInteraction;
use crate::model::browse::session::BrowseSession;
use crate::model::connection::cache::ConnectionCacheStore;
use crate::model::connection::error_state::ConnectionErrorState;
use crate::model::connection::list::ConnectionListItem;
use crate::model::connection::setup::ConnectionSetupState;
use crate::model::shared::confirm_dialog::ConfirmDialogState;
use crate::model::shared::flash_timer::FlashTimerStore;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::message::MessageState;
use crate::model::shared::modal::ModalState;
use crate::model::shared::ui_state::UiState;
use crate::model::sql_editor::modal::SqlModalContext;
use crate::model::sql_editor::query_history::QueryHistoryPickerState;

pub struct AppState {
    pub should_quit: bool,
    pub command_line_input: crate::model::shared::text_input::TextInputState,
    pub command_line_visible_width: usize,

    pub render_dirty: bool,

    pub session: BrowseSession,
    pub runtime: RuntimeState,
    pub ui: UiState,
    pub query: QueryExecution,
    pub sql_modal: SqlModalContext,
    pub messages: MessageState,
    pub er_preparation: super::er_state::ErPreparationState,
    pub connection_setup: ConnectionSetupState,
    pub connection_error: ConnectionErrorState,
    pub confirm_dialog: ConfirmDialogState,
    pub result_interaction: ResultInteraction,
    pub jsonb_detail: JsonbDetailState,
    pub query_history_picker: QueryHistoryPickerState,
    pub explain: ExplainContext,
    pub modal: ModalState,
    pub flash_timers: FlashTimerStore,
    pub connection_caches: ConnectionCacheStore,
    connections: Vec<ConnectionProfile>,
    service_entries: Vec<ServiceEntry>,
    connection_list_items: Vec<ConnectionListItem>,
}

impl AppState {
    pub fn new(project_name: String) -> Self {
        Self {
            should_quit: false,
            command_line_input: crate::model::shared::text_input::TextInputState::default(),
            command_line_visible_width: 70,
            render_dirty: true,
            session: BrowseSession::default(),
            runtime: RuntimeState::new(project_name),
            ui: UiState::new(),
            query: QueryExecution::default(),
            sql_modal: SqlModalContext::default(),
            messages: MessageState::default(),
            er_preparation: super::er_state::ErPreparationState::default(),
            connection_setup: ConnectionSetupState::default(),
            connection_error: ConnectionErrorState::default(),
            confirm_dialog: ConfirmDialogState::default(),
            result_interaction: ResultInteraction::default(),
            jsonb_detail: JsonbDetailState::default(),
            query_history_picker: QueryHistoryPickerState::default(),
            explain: ExplainContext::default(),
            modal: ModalState::default(),
            flash_timers: FlashTimerStore::default(),
            connection_caches: ConnectionCacheStore::default(),
            connections: Vec::new(),
            service_entries: Vec::new(),
            connection_list_items: Vec::new(),
        }
    }

    pub fn input_mode(&self) -> InputMode {
        self.modal.active_mode()
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.render_dirty = true;
    }

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

    pub fn clear_expired_timers(&mut self, now: Instant) {
        self.messages.clear_expired();
        self.query.clear_expired_highlight(now);
        self.result_interaction.clear_expired_flash(now);
        self.flash_timers.clear_expired(now);
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

    pub fn jsonb_detail_editor_visible_rows(&self) -> usize {
        self.ui.jsonb_detail_editor_visible_rows
    }

    pub fn tables(&self) -> Vec<&TableSummary> {
        self.session.tables()
    }

    pub fn filtered_tables(&self) -> Vec<&TableSummary> {
        let filter_lower = self.ui.table_picker.filter_input().content().to_lowercase();
        self.session
            .metadata()
            .map(|m| {
                m.table_summaries
                    .iter()
                    .filter(|t| t.qualified_name_lower().contains(&filter_lower))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn er_filtered_tables(&self) -> Vec<&TableSummary> {
        let filter_lower = self.ui.er_picker.filter_input().content().to_lowercase();
        self.session
            .metadata()
            .map(|m| {
                m.table_summaries
                    .iter()
                    .filter(|t| t.qualified_name_lower().contains(&filter_lower))
                    .collect()
            })
            .unwrap_or_default()
    }

    // --- Connection state getters ---

    pub fn connections(&self) -> &[ConnectionProfile] {
        &self.connections
    }

    pub fn service_entries(&self) -> &[ServiceEntry] {
        &self.service_entries
    }

    pub fn connection_list_items(&self) -> &[ConnectionListItem] {
        &self.connection_list_items
    }

    // --- Connection state setters (auto-rebuild connection_list_items) ---

    pub fn set_connections(&mut self, connections: Vec<ConnectionProfile>) {
        self.connections = connections;
        self.rebuild_connection_list();
    }

    pub fn set_service_entries(&mut self, entries: Vec<ServiceEntry>) {
        self.service_entries = entries;
        self.rebuild_connection_list();
    }

    pub fn set_connections_and_services(
        &mut self,
        connections: Vec<ConnectionProfile>,
        entries: Vec<ServiceEntry>,
    ) {
        self.connections = connections;
        self.service_entries = entries;
        self.rebuild_connection_list();
    }

    pub fn retain_connections<F: FnMut(&ConnectionProfile) -> bool>(&mut self, f: F) {
        self.connections.retain(f);
        self.rebuild_connection_list();
    }

    fn rebuild_connection_list(&mut self) {
        self.connection_list_items = crate::model::connection::list::build_connection_list(
            self.connections.len(),
            self.service_entries.len(),
        );
    }

    pub fn toggle_focus(&mut self) -> bool {
        self.ui.toggle_focus()
    }

    pub fn can_request_csv_export(&self) -> bool {
        !self.query.is_history_mode() && self.query.visible_result().is_some_and(|r| !r.is_error())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;
    use crate::domain::DatabaseMetadata;
    use crate::domain::QuerySource;
    use crate::domain::Table;
    use crate::model::er_state::ErStatus;
    use crate::model::shared::focused_pane::FocusedPane;
    use crate::update::action::Action;
    use crate::update::reduce_metadata;
    use rstest::rstest;
    fn make_state() -> AppState {
        AppState::new("test".to_string())
    }

    fn make_query_result(source: QuerySource) -> Arc<crate::domain::QueryResult> {
        Arc::new(crate::domain::QueryResult::success(
            "SELECT 1".to_string(),
            vec!["col".to_string()],
            vec![vec!["val".to_string()]],
            10,
            source,
        ))
    }

    fn make_metadata(table_summaries: Vec<TableSummary>) -> Arc<DatabaseMetadata> {
        Arc::new(DatabaseMetadata {
            database_name: "test".to_string(),
            schemas: vec![],
            table_summaries,
            fetched_at: Instant::now(),
        })
    }

    fn make_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: Vec::new(),
            primary_key: None,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
        }
    }

    mod pane_geometry {
        use super::*;

        #[test]
        fn result_rows_default_to_zero() {
            let state = make_state();

            let visible = state.result_visible_rows();

            assert_eq!(visible, 0);
        }

        #[rstest]
        #[case(10, 5)]
        #[case(15, 10)]
        #[case(20, 15)]
        #[case(30, 25)]
        fn result_rows_follow_pane_height(#[case] pane_height: u16, #[case] expected: usize) {
            let mut state = make_state();
            state.ui.result_pane_height = pane_height;

            let visible = state.result_visible_rows();

            assert_eq!(visible, expected);
        }

        #[test]
        fn result_rows_clamp_small_heights() {
            let mut state = make_state();
            state.ui.result_pane_height = 2;

            let visible = state.result_visible_rows();

            assert_eq!(visible, 0);
        }

        #[test]
        fn result_rows_stay_zero_at_minimum() {
            let mut state = make_state();
            state.ui.result_pane_height = 1;

            let visible = state.result_visible_rows();

            assert_eq!(visible, 0);
        }

        #[test]
        fn result_rows_scale_with_height() {
            let mut state = make_state();
            state.ui.result_pane_height = 50;

            let visible = state.result_visible_rows();

            assert_eq!(visible, 45);
        }

        #[test]
        fn inspector_ddl_rows_exceed_standard_rows() {
            let mut state = make_state();
            state.ui.inspector_pane_height = 20;

            let standard = state.inspector_visible_rows();
            let ddl = state.inspector_ddl_visible_rows();

            // DDL omits the standard header rows, so it exposes two more rows.
            assert_eq!(ddl - standard, 2);
        }

        #[rstest]
        #[case(10, 7)]
        #[case(15, 12)]
        #[case(20, 17)]
        fn inspector_ddl_rows_subtract_three(#[case] pane_height: u16, #[case] expected: usize) {
            let mut state = make_state();
            state.ui.inspector_pane_height = pane_height;

            let visible = state.inspector_ddl_visible_rows();

            assert_eq!(visible, expected);
        }

        #[test]
        fn inspector_ddl_rows_clamp_small_heights() {
            let mut state = make_state();
            state.ui.inspector_pane_height = 2;

            let visible = state.inspector_ddl_visible_rows();

            assert_eq!(visible, 0);
        }
    }

    mod table_selection {
        use super::*;

        #[test]
        fn empty_filter_returns_all() {
            let mut state = make_state();
            state.session.set_metadata(Some(make_metadata(vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ])));
            state.ui.table_picker.clear_filter();

            let filtered = state.filtered_tables();

            assert_eq!(filtered.len(), 2);
        }

        #[test]
        fn substring_filter_matches() {
            let mut state = make_state();
            state.session.set_metadata(Some(make_metadata(vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ])));
            state.ui.table_picker.insert_filter_str("user");

            let filtered = state.filtered_tables();

            assert_eq!(filtered.len(), 1);
            assert_eq!(filtered[0].name, "users");
        }

        #[test]
        fn filter_ignores_case() {
            let mut state = make_state();
            state
                .session
                .set_metadata(Some(make_metadata(vec![TableSummary::new(
                    "public".to_string(),
                    "Users".to_string(),
                    Some(100),
                    false,
                )])));
            state.ui.table_picker.insert_filter_str("user");

            let filtered = state.filtered_tables();

            assert_eq!(filtered.len(), 1);
        }

        #[test]
        fn selection_generation_starts_at_zero() {
            let state = make_state();

            assert_eq!(state.session.selection_generation(), 0);
        }

        #[test]
        fn selection_generation_increments_on_selection() {
            let mut state = make_state();

            let gen1 = state.session.selection_generation();
            let gen2 = state
                .session
                .select_table("public", "t1", &mut state.query.pagination);
            let gen3 = state
                .session
                .select_table("public", "t2", &mut state.query.pagination);

            assert_eq!(gen1, 0);
            assert_eq!(gen2, 1);
            assert_eq!(gen3, 2);
        }

        #[test]
        fn selection_generation_advances_after_reselection() {
            let mut state = make_state();

            let initial_gen = state.session.selection_generation();
            let current_gen =
                state
                    .session
                    .select_table("public", "users", &mut state.query.pagination);

            assert!(initial_gen < current_gen);
        }
    }

    mod sql_modal_lifecycle {
        use super::*;

        #[test]
        fn prefetch_queue_starts_empty() {
            let state = make_state();

            assert!(state.sql_modal.prefetch_queue().is_empty());
            assert!(!state.sql_modal.is_prefetch_started());
        }

        #[test]
        fn prefetch_queue_is_fifo() {
            let mut state = make_state();
            state.sql_modal.enqueue_prefetch("public.users".to_string());
            state
                .sql_modal
                .enqueue_prefetch("public.orders".to_string());

            let first = state.sql_modal.pop_prefetch();
            let second = state.sql_modal.pop_prefetch();

            assert_eq!(first, Some("public.users".to_string()));
            assert_eq!(second, Some("public.orders".to_string()));
        }

        #[test]
        fn prefetching_tables_track_in_flight() {
            let mut state = make_state();

            state.sql_modal.mark_prefetching("public.users".to_string());

            assert!(
                state
                    .sql_modal
                    .prefetching_tables()
                    .contains("public.users")
            );
            assert!(
                !state
                    .sql_modal
                    .prefetching_tables()
                    .contains("public.orders")
            );
        }

        #[test]
        fn failed_prefetch_tables_store_error_and_time() {
            let mut state = make_state();
            let now = Instant::now();

            state.sql_modal.record_prefetch_failure(
                "public.users".to_string(),
                crate::model::sql_editor::modal::FailedPrefetchEntry {
                    failed_at: now,
                    error: "connection timeout".to_string(),
                    retry_count: 0,
                },
            );

            assert!(
                state
                    .sql_modal
                    .failed_prefetch_tables()
                    .contains_key("public.users")
            );
            let entry = state
                .sql_modal
                .failed_prefetch_tables()
                .get("public.users")
                .unwrap();
            assert_eq!(entry.failed_at, now);
            assert_eq!(entry.error, "connection timeout");
        }
    }

    mod reload_metadata_reset {
        use super::*;

        fn prepare_state_for_reload() -> AppState {
            let mut state = make_state();
            state.session.begin_connecting("postgres://localhost/test");
            state.sql_modal.begin_prefetch();
            state.sql_modal.enqueue_prefetch("public.users".to_string());
            state
                .sql_modal
                .mark_prefetching("public.orders".to_string());
            state.sql_modal.record_prefetch_failure(
                "public.failed".to_string(),
                crate::model::sql_editor::modal::FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "timeout".to_string(),
                    retry_count: 0,
                },
            );
            state
        }

        #[test]
        fn resets_prefetch_state() {
            let mut state = prepare_state_for_reload();

            reduce_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert!(!state.sql_modal.is_prefetch_started());
            assert!(state.sql_modal.prefetch_queue().is_empty());
            assert!(state.sql_modal.prefetching_tables().is_empty());
            assert!(state.sql_modal.failed_prefetch_tables().is_empty());
        }

        #[test]
        fn resets_er_preparation() {
            let mut state = prepare_state_for_reload();
            state.er_preparation.set_status_for_test(ErStatus::Waiting);

            reduce_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
        }

        #[test]
        fn clears_stale_messages() {
            let mut state = prepare_state_for_reload();
            state.messages.last_error = Some("Old error".to_string());
            state.messages.last_success = Some("Old success".to_string());
            state.messages.expires_at = Some(Instant::now());

            assert!(state.messages.last_error.is_some());
            assert!(state.messages.last_success.is_some());
            assert!(state.messages.expires_at.is_some());

            reduce_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert!(state.messages.last_error.is_none());
            assert!(state.messages.last_success.is_none());
            assert!(state.messages.expires_at.is_none());
        }
    }

    mod ui_facade {
        use super::*;

        #[test]
        fn toggle_focus_enters_focus_mode() {
            let mut state = make_state();
            state.ui.focused_pane = FocusedPane::Explorer;

            let result = state.toggle_focus();

            assert!(result);
            assert!(state.ui.is_focus_mode());
            assert_eq!(state.ui.focused_pane, FocusedPane::Result);
            assert_eq!(
                state.ui.focus_mode.previous_pane(),
                Some(FocusedPane::Explorer)
            );
        }

        #[test]
        fn toggle_focus_restores_previous_pane() {
            let mut state = make_state();
            state.ui.focused_pane = FocusedPane::Inspector;
            state.toggle_focus();

            let result = state.toggle_focus();

            assert!(result);
            assert!(!state.ui.is_focus_mode());
            assert_eq!(state.ui.focused_pane, FocusedPane::Inspector);
        }

        #[test]
        fn csv_export_allowed_for_live_result() {
            let mut state = make_state();
            state
                .query
                .set_current_result(make_query_result(QuerySource::Preview));

            assert!(state.can_request_csv_export());
        }

        #[test]
        fn csv_export_blocked_in_history_mode() {
            let mut state = make_state();
            state
                .query
                .push_history(make_query_result(QuerySource::Adhoc));
            state.query.enter_history(0);

            assert!(!state.can_request_csv_export());
        }
    }

    mod local_state_regressions {
        use super::*;

        mod er_preparation {
            use super::*;

            #[test]
            fn defaults_to_idle() {
                let state = make_state();

                assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            }

            #[rstest]
            #[case(ErStatus::Waiting)]
            #[case(ErStatus::Rendering)]
            fn accepts_status(#[case] status: ErStatus) {
                let mut state = make_state();

                state.er_preparation.set_status_for_test(status);

                assert_eq!(state.er_preparation.status(), status);
            }
        }

        mod inspector_scroll_reset {
            use super::*;

            #[test]
            fn resets_to_zero_on_table_detail_loaded() {
                let mut state = make_state();
                let _ = state
                    .session
                    .select_table("public", "users", &mut state.query.pagination);
                let generation = state.session.selection_generation();
                state.ui.inspector_scroll_offset = 42;

                reduce_metadata(
                    &mut state,
                    &Action::TableDetailLoaded(Box::new(make_table_detail()), generation),
                    Instant::now(),
                );

                assert_eq!(state.ui.inspector_scroll_offset, 0);
            }

            #[test]
            fn offset_defaults_to_zero() {
                let state = make_state();

                assert_eq!(state.ui.inspector_scroll_offset, 0);
                assert!(state.session.table_detail().is_none());
            }
        }
    }

    mod connection_catalog {
        use super::*;
        use crate::domain::connection::{ConnectionProfile, SslMode};
        use crate::model::connection::list::ConnectionListItem;

        fn make_profile(name: &str) -> ConnectionProfile {
            ConnectionProfile::new(
                name,
                "localhost",
                5432,
                "test",
                "user",
                "pass",
                SslMode::Prefer,
            )
            .unwrap()
        }

        fn make_service(name: &str) -> crate::domain::connection::ServiceEntry {
            crate::domain::connection::ServiceEntry {
                service_name: name.to_string(),
                host: None,
                dbname: None,
                port: None,
                user: None,
            }
        }

        #[test]
        fn set_connections_rebuilds_list() {
            let mut state = make_state();

            state.set_connections(vec![make_profile("a"), make_profile("b")]);

            assert_eq!(state.connections().len(), 2);
            assert_eq!(
                state.connection_list_items(),
                &[
                    ConnectionListItem::Profile(0),
                    ConnectionListItem::Profile(1)
                ]
            );
        }

        #[test]
        fn set_service_entries_rebuilds_list() {
            let mut state = make_state();

            state.set_service_entries(vec![make_service("s1"), make_service("s2")]);

            assert_eq!(state.service_entries().len(), 2);
            assert_eq!(
                state.connection_list_items(),
                &[
                    ConnectionListItem::Service(0),
                    ConnectionListItem::Service(1)
                ]
            );
        }

        #[test]
        fn set_connections_and_services_rebuilds_combined_list() {
            let mut state = make_state();

            state.set_connections_and_services(
                vec![make_profile("p1")],
                vec![make_service("s1"), make_service("s2")],
            );

            assert_eq!(state.connections().len(), 1);
            assert_eq!(state.service_entries().len(), 2);
            assert_eq!(state.connection_list_items().len(), 3);
            assert_eq!(
                state.connection_list_items(),
                &[
                    ConnectionListItem::Profile(0),
                    ConnectionListItem::Service(0),
                    ConnectionListItem::Service(1),
                ]
            );
        }

        #[test]
        fn retain_rebuilds_list() {
            let mut state = make_state();
            let keep = make_profile("keep");
            let drop = make_profile("drop");
            let keep_id = keep.id.clone();

            state.set_connections(vec![keep, drop]);
            assert_eq!(state.connections().len(), 2);

            state.retain_connections(|c| c.id == keep_id);

            assert_eq!(state.connections().len(), 1);
            assert_eq!(state.connections()[0].id, keep_id);
            assert_eq!(
                state.connection_list_items(),
                &[ConnectionListItem::Profile(0)]
            );
        }

        #[test]
        fn clear_connections_empties_list() {
            let mut state = make_state();
            state.set_connections(vec![make_profile("a")]);
            assert_eq!(state.connections().len(), 1);

            state.set_connections(vec![]);

            assert!(state.connections().is_empty());
            assert!(state.connection_list_items().is_empty());
        }

        #[test]
        fn clear_service_entries_empties_list() {
            let mut state = make_state();
            state.set_service_entries(vec![make_service("s1")]);
            assert_eq!(state.service_entries().len(), 1);

            state.set_service_entries(vec![]);

            assert!(state.service_entries().is_empty());
            assert!(state.connection_list_items().is_empty());
        }
    }
}
