use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::explain_context::ExplainContext;
use crate::domain::connection::{ConnectionId, ConnectionProfile, ServiceEntry};
use crate::domain::{Column, DatabaseType, TableSummary, mysql_sql::mysql_export_plan};
use crate::model::browse::inspector_view_model::InspectorViewModel;
use crate::model::browse::json_detail::JsonDetailState;
use crate::model::browse::query_execution::{QueryExecution, VisibleResultKind};
use crate::model::browse::result_interaction::ResultInteraction;
use crate::model::browse::row_detail::RowDetailState;
use crate::model::browse::session::BrowseSession;
use crate::model::connection::cache::ConnectionCache;
use crate::model::connection::error_state::ConnectionErrorState;
use crate::model::connection::list::{self, ConnectionListItem};
use crate::model::connection::setup::ConnectionSetupState;
use crate::model::shared::confirm_dialog::ConfirmDialogState;
use crate::model::shared::detail_view::ReadOnlyDetailState;
use crate::model::shared::flash_timer::FlashTimerStore;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::message::MessageState;
use crate::model::shared::modal::ModalState;
use crate::model::shared::render_output::{
    BrowseLayout, DetailLayout, InputLayout, OverlayLayout, PickerLayouts, RenderOutput,
};
use crate::model::shared::settings::SettingsState;
use crate::model::shared::text_input::TextInputState;
use crate::model::shared::ui_state::{UiState, scroll_max_offset};
use crate::model::sql_editor::modal::SqlModalContext;
use crate::model::sql_editor::query_history::QueryHistoryPickerState;
use crate::model::sqlite::diagnostics::SqliteDiagnosticsState;
use crate::model::table_prefetch::TablePrefetchState;
use crate::policy::preview_cell_text::CellPresentationPolicy;
use crate::policy::sql::result_query::is_rerunnable_select;
use crate::policy::table_kind::max_explorer_table_label_width;
use crate::policy::write::inline_cell_edit::supports_inline_edit;
use crate::policy::write::write_guardrails::{
    PreviewWriteability, preview_writeability_for_result,
};
use crate::ports::outbound::DdlGenerator;

pub struct AppState {
    pub should_quit: bool,
    pub command_line_input: TextInputState,
    pub command_line_visible_width: usize,
    kill_buffer: Option<String>,

    pub render_dirty: bool,

    pub session: BrowseSession,
    project_name: String,
    service_file_path: Option<PathBuf>,
    pub ui: UiState,
    pub query: QueryExecution,
    pub sql_modal: SqlModalContext,
    pub table_prefetch: TablePrefetchState,
    pub messages: MessageState,
    pub er_preparation: super::er_state::ErPreparationState,
    pub connection_setup: ConnectionSetupState,
    pub connection_error: ConnectionErrorState,
    pub confirm_dialog: ConfirmDialogState,
    pub result_interaction: ResultInteraction,
    pub cell_detail: ReadOnlyDetailState,
    pub json_detail: JsonDetailState,
    pub row_detail: RowDetailState,
    pub query_history_picker: QueryHistoryPickerState,
    pub settings: SettingsState,
    pub sqlite_diagnostics: SqliteDiagnosticsState,
    pub explain: ExplainContext,
    pub modal: ModalState,
    pub flash_timers: FlashTimerStore,
    pub connection_caches: HashMap<ConnectionId, ConnectionCache>,
    connections: Vec<ConnectionProfile>,
    service_entries: Vec<ServiceEntry>,
    connection_list_items: Vec<ConnectionListItem>,
}

impl AppState {
    pub fn new(project_name: String) -> Self {
        Self {
            should_quit: false,
            command_line_input: TextInputState::default(),
            command_line_visible_width: 70,
            kill_buffer: None,
            render_dirty: true,
            session: BrowseSession::default(),
            project_name,
            service_file_path: None,
            ui: UiState::new(),
            query: QueryExecution::default(),
            sql_modal: SqlModalContext::default(),
            table_prefetch: TablePrefetchState::default(),
            messages: MessageState::default(),
            er_preparation: super::er_state::ErPreparationState::default(),
            connection_setup: ConnectionSetupState::default(),
            connection_error: ConnectionErrorState::default(),
            confirm_dialog: ConfirmDialogState::default(),
            result_interaction: ResultInteraction::default(),
            cell_detail: ReadOnlyDetailState::default(),
            json_detail: JsonDetailState::default(),
            row_detail: RowDetailState::default(),
            query_history_picker: QueryHistoryPickerState::default(),
            settings: SettingsState::default(),
            sqlite_diagnostics: SqliteDiagnosticsState::default(),
            explain: ExplainContext::default(),
            modal: ModalState::default(),
            flash_timers: FlashTimerStore::default(),
            connection_caches: HashMap::default(),
            connections: Vec::new(),
            service_entries: Vec::new(),
            connection_list_items: Vec::new(),
        }
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn service_file_path(&self) -> Option<&Path> {
        self.service_file_path.as_deref()
    }

    pub fn set_service_file_path(&mut self, path: Option<PathBuf>) {
        self.service_file_path = path;
    }

    pub fn input_mode(&self) -> InputMode {
        self.modal.active_mode()
    }

    pub fn record_kill(&mut self, text: String) {
        if !text.is_empty() {
            self.kill_buffer = Some(text);
        }
    }

    pub fn kill_buffer(&self) -> Option<&str> {
        self.kill_buffer.as_deref()
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.render_dirty = true;
    }

    #[inline]
    pub fn clear_dirty(&mut self) {
        self.render_dirty = false;
    }

    pub fn clear_expired_timers(&mut self, now: Instant) {
        self.messages.clear_expired_at(now);
        self.query.clear_expired_highlight(now);
        self.result_interaction.clear_expired_flash(now);
        self.flash_timers.clear_expired(now);
    }

    /// Applies layout data produced during a draw. Inspector viewport plans are
    /// skipped in focus mode to keep the pre-focus plan restorable.
    pub fn apply_render_output(&mut self, output: RenderOutput) {
        self.apply_browse_layout(output.browse);
        self.apply_input_layout(output.input);
        self.apply_picker_layouts(output.pickers);
        self.apply_detail_layout(output.details);
        self.apply_overlay_layout(output.overlays);
    }

    fn apply_browse_layout(&mut self, layout: BrowseLayout) {
        if !self.ui.is_focus_mode() {
            self.ui
                .set_inspector_viewport_plan(layout.inspector.viewport_plan);
        }
        self.ui
            .set_result_viewport_plan(layout.result.viewport_plan);
        self.ui.set_result_widths_cache(layout.result.widths_cache);
        self.ui
            .set_explorer_pane_height(layout.explorer.pane_height);
        self.ui
            .set_explorer_content_width(layout.explorer.content_width);
        let max_name_width = max_explorer_table_label_width(
            self.tables(),
            self.session.active_database_type_or_default(),
        );
        let max_offset = scroll_max_offset(max_name_width, self.ui.explorer_content_width());
        self.ui
            .set_explorer_horizontal_offset(self.ui.explorer_horizontal_offset().min(max_offset));
        self.ui
            .set_inspector_pane_height(layout.inspector.pane_height);
        self.ui.set_result_pane_height(layout.result.pane_height);
    }

    fn apply_input_layout(&mut self, layout: InputLayout) {
        if let Some(width) = layout.command_line_visible_width {
            self.command_line_visible_width = width;
        }
    }

    fn apply_picker_layouts(&mut self, layouts: PickerLayouts) {
        if let Some(height) = layouts.connection_list_pane_height {
            self.ui.set_connection_list_pane_height(height);
        }
        if let Some(table) = layouts.table {
            self.ui
                .table_picker_mut()
                .set_pane_height(table.pane_height);
            self.ui
                .table_picker_mut()
                .set_filter_visible_width(table.filter_visible_width);
        }
        if let Some(er) = layouts.er {
            self.ui.er_picker_mut().set_pane_height(er.pane_height);
            self.ui
                .er_picker_mut()
                .set_filter_visible_width(er.filter_visible_width);
        }
        if let Some(query_history) = layouts.query_history {
            self.query_history_picker
                .set_pane_height(query_history.pane_height);
            self.query_history_picker
                .set_filter_visible_width(query_history.filter_visible_width);
        }
    }

    fn apply_detail_layout(&mut self, layout: DetailLayout) {
        if let Some(json) = layout.json {
            self.ui
                .set_json_detail_editor_visible_rows(json.editor_visible_rows);
            self.json_detail
                .editor_mut()
                .update_scroll(json.editor_visible_rows);
        }
        if let Some(viewport) = layout.cell {
            self.cell_detail
                .set_viewport_metrics(viewport.visible_rows, viewport.viewport_width);
        }
        if let Some(row) = layout.row {
            self.ui.row_detail_content_visible_rows = row.visible_rows;
            self.ui.row_detail_content_visible_columns = row.visible_columns;
            self.row_detail.clamp_scroll(
                self.ui.row_detail_content_visible_rows,
                self.ui.row_detail_content_visible_columns,
            );
        }
    }

    fn apply_overlay_layout(&mut self, layout: OverlayLayout) {
        self.confirm_dialog.apply_preview_metrics(
            layout.confirm_preview.viewport_height,
            layout.confirm_preview.content_height,
            layout.confirm_preview.scroll,
        );
        if let Some(height) = layout.explain_compare_viewport_height {
            self.explain.set_compare_viewport_height(height);
        }
        if let (Some(content), Some(viewport)) = (
            layout.sqlite_diagnostics_content_line_count,
            layout.sqlite_diagnostics_viewport_height,
        ) {
            self.sqlite_diagnostics
                .apply_viewport_metrics(content, viewport);
        }
    }

    pub fn result_visible_rows(&self) -> usize {
        self.ui.result_visible_rows()
    }

    pub fn inspector_view_model(&self, ddl_generator: &dyn DdlGenerator) -> InspectorViewModel {
        InspectorViewModel::build_with_detail_state(
            &self.session.active_engine_feature_profile(),
            self.ui.inspector_tab(),
            self.session.table_detail_state(),
            self.session.active_database_type_or_default(),
            ddl_generator,
        )
    }

    pub fn row_detail_content_visible_rows(&self) -> usize {
        self.ui.row_detail_content_visible_rows
    }

    pub fn row_detail_content_visible_columns(&self) -> usize {
        self.ui.row_detail_content_visible_columns
    }

    pub fn tables(&self) -> &[TableSummary] {
        self.session.tables()
    }

    pub fn filtered_tables(&self) -> Vec<&TableSummary> {
        let filter_lower = self
            .ui
            .table_picker()
            .filter_input()
            .content()
            .to_lowercase();
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
        let filter_lower = self.ui.er_picker().filter_input().content().to_lowercase();
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
        self.connection_list_items =
            list::build_connection_list(self.connections.len(), self.service_entries.len());
    }

    pub fn can_retry_connection_error(&self) -> bool {
        !self.connection_error.is_save_and_connect_failure()
            && (self.session.has_pending_connection_switch()
                || !self.session.can_reenter_connection_setup()
                || (self.session.active_database_type() == Some(DatabaseType::MySQL)
                    && self.connection_error.can_retry()))
    }

    pub fn can_request_csv_export(&self) -> bool {
        let Some(result) = self.query.visible_result() else {
            return false;
        };
        if result.is_error() {
            return false;
        }
        match self.session.active_database_type() {
            Some(DatabaseType::SQLite) => true,
            Some(DatabaseType::MySQL) => mysql_export_plan(&result.query).is_some(),
            _ => is_rerunnable_select(&result.query),
        }
    }

    pub fn visible_preview_target_read_only_reason(&self) -> Option<&'static str> {
        if !self.query.can_edit_visible_result() {
            return None;
        }
        let table_detail = self.session.table_detail()?;
        if !self.query.pagination.matches_table(table_detail) {
            return None;
        }
        let result = self.query.visible_result()?;
        match preview_writeability_for_result(table_detail, result) {
            PreviewWriteability::Writable => None,
            PreviewWriteability::ReadOnly(reason) => Some(reason),
            PreviewWriteability::MissingStableRowIdentity => Some("table without PRIMARY KEY"),
        }
    }

    pub fn can_write_visible_preview(&self) -> bool {
        self.query.can_edit_visible_result()
            && self.visible_preview_target_read_only_reason().is_none()
    }

    pub fn visible_preview_column(&self, col_idx: usize) -> Option<&Column> {
        if self.query.visible_result_kind() != VisibleResultKind::LivePreview {
            return None;
        }

        let result = self.query.visible_result()?;
        let column_name = result.columns.get(col_idx)?;
        let table_detail = self.session.table_detail()?;
        table_detail
            .columns
            .iter()
            .find(|column| column.name == *column_name)
    }

    pub fn can_edit_selected_cell(&self) -> bool {
        let Some(row_idx) = self.result_interaction.selection().row() else {
            return false;
        };
        let Some(col_idx) = self.result_interaction.selection().cell() else {
            return false;
        };
        if !self.can_write_visible_preview() {
            return false;
        }

        let Some(result) = self.query.visible_result() else {
            return false;
        };
        if row_idx >= result.values().len() {
            return false;
        }
        let Some(value) = result.value_at(row_idx, col_idx) else {
            return false;
        };

        let Some(column) = self.visible_preview_column(col_idx) else {
            return false;
        };
        let is_primary_key = column.is_primary_key()
            || self
                .session
                .table_detail()
                .and_then(|table| table.primary_key.as_ref())
                .is_some_and(|primary_key| primary_key.iter().any(|name| name == &column.name));
        if is_primary_key || column.is_read_only() {
            return false;
        }

        let policy = CellPresentationPolicy::new(
            self.session.active_database_type_or_default(),
            column.data_type.as_str(),
            value.as_str().unwrap_or_default(),
        );
        if policy.uses_json_detail_modal() {
            return true;
        }

        supports_inline_edit(self.session.active_database_type_or_default(), value)
    }

    /// True when a run-scoped async response no longer belongs to the active
    /// connection and query run, and must be dropped without touching state.
    pub fn is_stale_query_run(&self, dsn: &str, run_id: u64) -> bool {
        !self.session.dsn_matches(dsn) || !self.query.is_current_run(run_id)
    }

    pub fn is_stale_explain_run(&self, database_generation: u64, run_id: u64) -> bool {
        !self.query.is_current_run(run_id)
            || self.session.database_generation() != database_generation
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support;

    use std::sync::Arc;
    use std::time::Instant;

    use super::*;
    use crate::domain::{
        ColumnAttributes, ConnectionId, DatabaseMetadata, DatabaseType, QueryResult, QuerySource,
        QueryValue, Table, TableKind, TableKindInfo,
    };
    use crate::model::browse::inspector_view_model::InspectorLoadState;
    use crate::model::browse::row_detail::RowDetailState;
    use crate::model::connection::error::{
        ConnectionErrorInfo, test_support as connection_error_test_support,
    };
    use crate::model::er_state::ErStatus;
    use crate::model::shared::render_output::{
        ConfirmPreviewLayout, InspectorLayout, RowDetailLayout,
    };
    use crate::model::shared::viewport::ViewportPlan;
    use crate::model::table_prefetch::FailedPrefetchEntry;
    use crate::services::AppServices;
    use crate::update::action::Action;
    use crate::update::dispatch_metadata;
    use rstest::rstest;
    fn make_state() -> AppState {
        AppState::new("test".to_string())
    }

    #[test]
    fn new_stores_project_name() {
        let state = AppState::new("my_project".to_string());

        assert_eq!(state.project_name(), "my_project");
    }

    fn activate_postgres_connection(state: &mut AppState, dsn: &str) {
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "postgres",
            DatabaseType::PostgreSQL,
            dsn,
        );
    }

    fn make_query_result(source: QuerySource) -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT 1".to_string(),
            vec!["col".to_string()],
            vec![vec!["val".to_string()]],
            10,
            source,
        ))
    }

    fn make_metadata(table_summaries: Vec<TableSummary>) -> Arc<DatabaseMetadata> {
        let mut metadata = DatabaseMetadata::new("test".to_string());
        metadata.table_summaries = table_summaries;
        Arc::new(metadata)
    }

    fn make_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            ..test_support::table::minimal("", "")
        }
    }

    mod connection_error_retry {
        use super::*;

        fn active_connection_state(database_type: DatabaseType, dsn: &str) -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                database_type,
                dsn,
            );
            state
        }

        fn retryable_error() -> ConnectionErrorInfo {
            connection_error_test_support::from_parts(
                "Connection timed out",
                "Check network connectivity",
                true,
                "connection timed out",
            )
        }

        #[test]
        fn allows_retry_for_active_mysql_retryable_error() {
            let mut state = active_connection_state(
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
            );
            state.connection_error.set_error(retryable_error());

            assert!(state.can_retry_connection_error());
        }

        #[test]
        fn hides_retry_for_non_mysql_retryable_error() {
            let mut state =
                active_connection_state(DatabaseType::PostgreSQL, "postgres://localhost/app");
            state.connection_error.set_error(retryable_error());

            assert!(!state.can_retry_connection_error());
        }

        #[test]
        fn hides_retry_for_save_and_connect_failure() {
            let mut state = active_connection_state(
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
            );
            state
                .connection_error
                .set_save_and_connect_error(retryable_error());

            assert!(!state.can_retry_connection_error());
        }

        #[test]
        fn keeps_retry_for_pending_connection_switch() {
            let mut state = active_connection_state(
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/old?ssl-mode=PREFERRED",
            );
            let target_id = ConnectionId::from_string("mysql-new");
            let _ = state.session.begin_mysql_connection_probe(
                &target_id,
                "mysql-new",
                "mysql://user@localhost:3306/new?ssl-mode=PREFERRED",
                Some("new"),
            );
            state
                .connection_error
                .set_error(connection_error_test_support::from_parts(
                    "Connection failed",
                    "See details for more information",
                    false,
                    "connection failed",
                ));

            assert!(state.can_retry_connection_error());
        }
    }

    #[rstest]
    fn inspector_view_model_exposes_not_selected_for_each_database_type(
        #[values(DatabaseType::MySQL, DatabaseType::PostgreSQL, DatabaseType::SQLite)]
        database_type: DatabaseType,
    ) {
        let mut state = make_state();
        activate_database_connection(&mut state, database_type);

        let services = AppServices::stub();
        let view_model = state.inspector_view_model(services.ddl_generator.as_ref());

        assert_eq!(
            view_model.load_state(),
            &InspectorLoadState::NoTableSelected
        );
        assert_eq!(view_model.empty_state(), None);
    }

    #[rstest]
    fn inspector_view_model_exposes_loading_for_each_database_type(
        #[values(DatabaseType::MySQL, DatabaseType::PostgreSQL, DatabaseType::SQLite)]
        database_type: DatabaseType,
    ) {
        let mut state = make_state();
        activate_database_connection(&mut state, database_type);
        let _ = state
            .session
            .select_table("public", "users", &mut state.query);

        let services = AppServices::stub();
        let view_model = state.inspector_view_model(services.ddl_generator.as_ref());

        assert_eq!(view_model.load_state(), &InspectorLoadState::Loading);
        assert_eq!(view_model.empty_state(), None);
    }

    #[rstest]
    fn inspector_view_model_exposes_error_for_each_database_type(
        #[values(DatabaseType::MySQL, DatabaseType::PostgreSQL, DatabaseType::SQLite)]
        database_type: DatabaseType,
    ) {
        let mut state = make_state();
        activate_database_connection(&mut state, database_type);
        let _ = state
            .session
            .select_table("public", "users", &mut state.query);
        assert!(state.session.mark_table_detail_failed(
            state.session.selection_generation(),
            "permission denied".to_string()
        ));

        let services = AppServices::stub();
        let view_model = state.inspector_view_model(services.ddl_generator.as_ref());

        assert_eq!(
            view_model.load_state(),
            &InspectorLoadState::Error("permission denied".to_string())
        );
        assert_eq!(view_model.empty_state(), None);
    }

    #[rstest]
    fn inspector_view_model_exposes_loaded_for_each_database_type(
        #[values(DatabaseType::MySQL, DatabaseType::PostgreSQL, DatabaseType::SQLite)]
        database_type: DatabaseType,
    ) {
        let mut state = make_state();
        activate_database_connection(&mut state, database_type);
        let generation = state
            .session
            .select_table("public", "users", &mut state.query);
        assert!(
            state
                .session
                .set_table_detail(make_table_detail(), generation)
        );

        let services = AppServices::stub();
        let view_model = state.inspector_view_model(services.ddl_generator.as_ref());

        assert_eq!(view_model.load_state(), &InspectorLoadState::Loaded);
        assert!(view_model.section().is_some());
    }

    fn activate_database_connection(state: &mut AppState, database_type: DatabaseType) {
        let (name, dsn) = match database_type {
            DatabaseType::MySQL => ("mysql", "mysql://localhost/test"),
            DatabaseType::PostgreSQL => ("postgres", "postgres://localhost/test"),
            DatabaseType::SQLite => ("sqlite", "sqlite:///tmp/app.db"),
        };
        state
            .session
            .activate_connection_with_dsn(&ConnectionId::new(), name, database_type, dsn);
    }

    fn test_column(
        name: &str,
        data_type: &str,
        attributes: ColumnAttributes,
        ordinal_position: i32,
    ) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes,
            comment: None,
            ordinal_position,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }

    mod sqlite_numeric_edit {
        use super::*;

        fn sqlite_numeric_edit_state() -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    "SELECT id, score FROM users".to_string(),
                    vec!["id".to_string(), "score".to_string()],
                    vec![vec![
                        QueryValue::SqlLiteral("1".to_string()),
                        QueryValue::SqlLiteral("42".to_string()),
                    ]],
                    10,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("main", "users");

            let mut table = test_support::table::minimal("main", "users");
            table.columns = vec![
                test_column("id", "INTEGER", ColumnAttributes::PRIMARY_KEY, 1),
                test_column("score", "REAL", ColumnAttributes::NULLABLE, 2),
            ];
            table.primary_key = Some(vec!["id".to_string()]);
            state.session.set_table_detail_raw(Some(table));
            state.result_interaction.activate_cell(0, 1);
            state
        }

        #[test]
        fn can_edit_selected_cell_allows_sqlite_numeric_literal() {
            let state = sqlite_numeric_edit_state();

            assert!(state.can_edit_selected_cell());
        }
    }

    mod blob_cell {
        use super::*;

        fn blob_cell_state() -> AppState {
            let mut state = make_state();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    "SELECT id, payload FROM users".to_string(),
                    vec!["id".to_string(), "payload".to_string()],
                    vec![vec![QueryValue::text("1"), QueryValue::Blob(vec![0, 255])]],
                    10,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");

            let mut table = test_support::table::minimal("public", "users");
            table.columns = vec![
                test_column("id", "INTEGER", ColumnAttributes::PRIMARY_KEY, 1),
                test_column("payload", "BLOB", ColumnAttributes::NULLABLE, 2),
            ];
            table.primary_key = Some(vec!["id".to_string()]);
            state.session.set_table_detail_raw(Some(table));
            state.result_interaction.activate_cell(0, 1);
            state
        }

        #[test]
        fn can_edit_selected_cell_rejects_blob_cell() {
            let state = blob_cell_state();

            assert!(!state.can_edit_selected_cell());
        }
    }

    mod mysql_hidden_primary_key {
        use super::*;

        fn mysql_hidden_primary_key_state() -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "mysql",
                DatabaseType::MySQL,
                "mysql://localhost/test",
            );
            state.query.set_current_result(Arc::new(
                QueryResult::success_with_values(
                    "SELECT `payload` FROM `sabiql_test`.`users`".to_string(),
                    vec!["payload".to_string()],
                    vec![vec![QueryValue::text("before")]],
                    10,
                    QuerySource::Preview,
                )
                .with_explicit_row_identity(
                    vec!["id".to_string()],
                    vec![vec![QueryValue::SqlLiteral("1".to_string())]],
                ),
            ));
            state
                .query
                .pagination
                .reset_for_table("sabiql_test", "users");

            let mut table = test_support::table::minimal("sabiql_test", "users");
            table.columns = vec![
                test_column(
                    "id",
                    "INT",
                    ColumnAttributes::PRIMARY_KEY
                        | ColumnAttributes::HIDDEN
                        | ColumnAttributes::READ_ONLY,
                    1,
                ),
                test_column("payload", "TEXT", ColumnAttributes::empty(), 2),
            ];
            table.primary_key = Some(vec!["id".to_string()]);
            state.session.set_table_detail_raw(Some(table));
            state.result_interaction.activate_cell(0, 0);
            state
        }

        #[test]
        fn can_edit_selected_cell_maps_visible_column_by_name_after_hidden_primary_key() {
            let state = mysql_hidden_primary_key_state();

            assert!(state.can_edit_selected_cell());
        }
    }

    mod mysql_hidden_primary_key_between_columns {
        use super::*;

        fn mysql_hidden_primary_key_between_columns_state() -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "mysql",
                DatabaseType::MySQL,
                "mysql://localhost/test",
            );
            state.query.set_current_result(Arc::new(
                QueryResult::success_with_values(
                    "SELECT `name`, `payload` FROM `sabiql_test`.`users`".to_string(),
                    vec!["name".to_string(), "payload".to_string()],
                    vec![vec![QueryValue::text("Alice"), QueryValue::text("before")]],
                    10,
                    QuerySource::Preview,
                )
                .with_explicit_row_identity(
                    vec!["id".to_string()],
                    vec![vec![QueryValue::SqlLiteral("1".to_string())]],
                ),
            ));
            state
                .query
                .pagination
                .reset_for_table("sabiql_test", "users");

            let mut table = test_support::table::minimal("sabiql_test", "users");
            table.columns = vec![
                test_column("name", "VARCHAR", ColumnAttributes::empty(), 1),
                test_column(
                    "id",
                    "INT",
                    ColumnAttributes::PRIMARY_KEY
                        | ColumnAttributes::HIDDEN
                        | ColumnAttributes::READ_ONLY,
                    2,
                ),
                test_column("payload", "TEXT", ColumnAttributes::empty(), 3),
            ];
            table.primary_key = Some(vec!["id".to_string()]);
            state.session.set_table_detail_raw(Some(table));
            state.result_interaction.activate_cell(0, 1);
            state
        }

        #[test]
        fn can_edit_selected_cell_maps_visible_column_by_name_when_hidden_primary_key_is_between_columns()
         {
            let state = mysql_hidden_primary_key_between_columns_state();

            assert!(state.can_edit_selected_cell());
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
            state.ui.set_result_pane_height(pane_height);

            let visible = state.result_visible_rows();

            assert_eq!(visible, expected);
        }

        #[test]
        fn result_rows_clamp_small_heights() {
            let mut state = make_state();
            state.ui.set_result_pane_height(2);

            let visible = state.result_visible_rows();

            assert_eq!(visible, 0);
        }

        #[test]
        fn result_rows_stay_zero_at_minimum() {
            let mut state = make_state();
            state.ui.set_result_pane_height(1);

            let visible = state.result_visible_rows();

            assert_eq!(visible, 0);
        }

        #[test]
        fn result_rows_scale_with_height() {
            let mut state = make_state();
            state.ui.set_result_pane_height(50);

            let visible = state.result_visible_rows();

            assert_eq!(visible, 45);
        }

        #[test]
        fn row_detail_scroll_offset_clamps_on_resize() {
            let mut state = make_state();
            state.row_detail = RowDetailState::open(&["id".to_string()], &["1".to_string()]);
            state.row_detail.scroll_down_by(10, 1);
            let output = RenderOutput {
                details: DetailLayout {
                    row: Some(RowDetailLayout {
                        visible_rows: 3,
                        visible_columns: 80,
                    }),
                    ..DetailLayout::default()
                },
                ..RenderOutput::default()
            };

            state.apply_render_output(output);

            assert_eq!(
                state.row_detail.scroll_offset(),
                state.row_detail.line_count().saturating_sub(3)
            );
        }

        #[test]
        fn focus_mode_preserves_inspector_viewport_plan() {
            let mut state = make_state();
            state.ui.set_inspector_viewport_plan(ViewportPlan {
                column_count: 2,
                ..ViewportPlan::default()
            });
            state.ui.toggle_focus();
            let output = RenderOutput {
                browse: BrowseLayout {
                    inspector: InspectorLayout {
                        viewport_plan: ViewportPlan {
                            column_count: 9,
                            ..ViewportPlan::default()
                        },
                        pane_height: 30,
                    },
                    ..BrowseLayout::default()
                },
                ..RenderOutput::default()
            };

            state.apply_render_output(output);

            assert_eq!(state.ui.inspector_viewport_plan().column_count, 2);
            assert_eq!(state.ui.inspector_pane_height(), 30);
        }
    }

    mod render_output {
        use super::*;

        #[test]
        fn confirm_preview_layout_is_applied() {
            let mut state = make_state();
            let output = RenderOutput {
                overlays: OverlayLayout {
                    confirm_preview: ConfirmPreviewLayout {
                        viewport_height: Some(10),
                        content_height: Some(25),
                        scroll: 4,
                    },
                    ..OverlayLayout::default()
                },
                ..RenderOutput::default()
            };

            state.apply_render_output(output);

            assert_eq!(state.confirm_dialog.preview_viewport_height, Some(10));
            assert_eq!(state.confirm_dialog.preview_content_height, Some(25));
            assert_eq!(state.confirm_dialog.preview_scroll, 4);
        }

        #[test]
        fn confirm_preview_layout_is_reset_when_not_rendered() {
            let mut state = make_state();
            state.confirm_dialog.preview_viewport_height = Some(10);
            state.confirm_dialog.preview_content_height = Some(25);
            state.confirm_dialog.preview_scroll = 4;

            state.apply_render_output(RenderOutput::default());

            assert_eq!(state.confirm_dialog.preview_viewport_height, None);
            assert_eq!(state.confirm_dialog.preview_content_height, None);
            assert_eq!(state.confirm_dialog.preview_scroll, 0);
        }

        #[test]
        fn explain_compare_height_changes_only_when_present() {
            let mut state = make_state();
            let output = RenderOutput {
                overlays: OverlayLayout {
                    explain_compare_viewport_height: Some(12),
                    ..OverlayLayout::default()
                },
                ..RenderOutput::default()
            };

            state.apply_render_output(output);
            state.apply_render_output(RenderOutput::default());

            assert_eq!(state.explain.compare_viewport_height, Some(12));
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
            state.ui.table_picker_mut().clear_filter();

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
            state.ui.table_picker_mut().insert_filter_str("user");

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
            state.ui.table_picker_mut().insert_filter_str("user");

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
            let gen2 = state.session.select_table("public", "t1", &mut state.query);
            let gen3 = state.session.select_table("public", "t2", &mut state.query);

            assert_eq!(gen1, 0);
            assert_eq!(gen2, 1);
            assert_eq!(gen3, 2);
        }

        #[test]
        fn selection_generation_advances_after_reselection() {
            let mut state = make_state();

            let initial_gen = state.session.selection_generation();
            let current_gen = state
                .session
                .select_table("public", "users", &mut state.query);

            assert!(initial_gen < current_gen);
        }
    }

    mod table_prefetch_lifecycle {
        use super::*;

        #[test]
        fn prefetch_queue_starts_empty() {
            let state = make_state();

            assert!(!state.table_prefetch.has_pending_prefetch());
            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
        }

        #[test]
        fn prefetch_queue_is_fifo() {
            let mut state = make_state();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
            state
                .table_prefetch
                .queue_table_prefetch("public.orders".to_string());

            let first = state.table_prefetch.take_next_prefetch();
            let second = state.table_prefetch.take_next_prefetch();

            assert_eq!(first, Some("public.users".to_string()));
            assert_eq!(second, Some("public.orders".to_string()));
        }

        #[test]
        fn prefetching_tables_track_in_flight() {
            let mut state = make_state();

            state
                .table_prefetch
                .start_table_prefetch("public.users".to_string());

            assert!(state.table_prefetch.is_table_prefetching("public.users"));
            assert!(!state.table_prefetch.is_table_prefetching("public.orders"));
        }

        #[test]
        fn failed_prefetch_tables_store_error_and_time() {
            let mut state = make_state();
            let now = Instant::now();

            state.table_prefetch.fail_table_prefetch(
                "public.users".to_string(),
                FailedPrefetchEntry {
                    failed_at: now,
                    error: "connection timeout".to_string(),
                    retry_count: 0,
                },
            );

            let entry = state
                .table_prefetch
                .failed_prefetch("public.users")
                .unwrap();
            assert_eq!(entry.failed_at, now);
            assert_eq!(entry.error, "connection timeout");
        }
    }

    mod reload_metadata_reset {
        use super::*;

        fn prepare_state_for_reload() -> AppState {
            let mut state = make_state();
            activate_postgres_connection(&mut state, "postgres://localhost/test");
            let _ = state.table_prefetch.begin_er_prefetch();
            state
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
            state
                .table_prefetch
                .start_table_prefetch("public.orders".to_string());
            state.table_prefetch.fail_table_prefetch(
                "public.failed".to_string(),
                FailedPrefetchEntry {
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

            dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert!(state.table_prefetch.active_prefetch_run_id().is_none());
            assert!(!state.table_prefetch.has_pending_prefetch());
            assert_eq!(state.table_prefetch.prefetch_in_flight_count(), 0);
            assert!(
                state
                    .table_prefetch
                    .failed_prefetch("public.failed")
                    .is_none()
            );
        }

        #[test]
        fn resets_er_preparation() {
            let mut state = prepare_state_for_reload();
            let _ = state.er_preparation.start_waiting_run();

            dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
        }

        #[test]
        fn clears_stale_messages() {
            let mut state = prepare_state_for_reload();
            state.messages.set_error("Old error".to_string());

            assert!(state.messages.last_error().is_some());
            assert!(state.messages.expires_at().is_none());

            dispatch_metadata(&mut state, &Action::ReloadMetadata, Instant::now());

            assert!(state.messages.last_error().is_none());
            assert!(state.messages.expires_at().is_none());
        }
    }

    mod ui_facade {
        use super::*;

        fn sqlite_preview_state_with_table(mut table: Table) -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );
            table.schema = "main".to_string();
            table.name = "logs".to_string();
            state.session.set_table_detail_raw(Some(table));
            state
                .query
                .set_current_result(make_query_result(QuerySource::Preview));
            state.query.pagination.reset_for_table("main", "logs");
            state
        }

        fn postgres_preview_state_with_table(mut table: Table) -> AppState {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "postgres",
                DatabaseType::PostgreSQL,
                "postgres://localhost/app",
            );
            table.schema = "public".to_string();
            table.name = "logs".to_string();
            state.session.set_table_detail_raw(Some(table));
            state
                .query
                .set_current_result(make_query_result(QuerySource::Preview));
            state.query.pagination.reset_for_table("public", "logs");
            state
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
        fn csv_export_blocked_for_mutating_result_query() {
            let mut state = make_state();
            let result = QueryResult::success(
                "UPDATE users SET name = 'b' WHERE id = 1 RETURNING id".to_string(),
                vec!["id".to_string()],
                vec![vec!["1".to_string()]],
                10,
                QuerySource::Adhoc,
            );
            state.query.set_current_result(Arc::new(result));

            assert!(!state.can_request_csv_export());
        }

        #[rstest]
        #[case("SELECT id FROM users")]
        #[case("TABLE users")]
        #[case("SHOW TABLES")]
        #[case("DESCRIBE users")]
        fn mysql_csv_export_allows_supported_result_queries(#[case] query: &str) {
            let mut state = make_state();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "mysql",
                DatabaseType::MySQL,
                "mysql://localhost/test",
            );
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    query.to_string(),
                    vec!["column".to_string()],
                    vec![vec!["value".to_string()]],
                    10,
                    QuerySource::Adhoc,
                )));

            assert!(state.can_request_csv_export());
        }

        #[test]
        fn sqlite_table_without_primary_key_is_read_only() {
            let state = sqlite_preview_state_with_table(test_support::table::minimal("", ""));

            assert!(!state.can_write_visible_preview());
            assert_eq!(
                state.visible_preview_target_read_only_reason(),
                Some("table without PRIMARY KEY")
            );
        }

        #[rstest]
        #[case(TableKind::View, "view")]
        #[case(TableKind::Virtual, "virtual table")]
        fn sqlite_non_rowid_targets_are_read_only(#[case] kind: TableKind, #[case] reason: &str) {
            let mut table = test_support::table::minimal("", "");
            table.kind_info = TableKindInfo {
                kind,
                ..TableKindInfo::default()
            };
            let state = sqlite_preview_state_with_table(table);

            assert!(!state.can_write_visible_preview());
            assert_eq!(
                state.visible_preview_target_read_only_reason(),
                Some(reason)
            );
        }

        #[test]
        fn sqlite_without_rowid_table_with_primary_key_can_write_preview() {
            let mut table = test_support::table::minimal("", "");
            table.primary_key = Some(vec!["id".to_string()]);
            table.kind_info.without_rowid = true;
            let state = sqlite_preview_state_with_table(table);

            assert!(state.can_write_visible_preview());
            assert_eq!(state.visible_preview_target_read_only_reason(), None);
        }

        #[test]
        fn sqlite_table_without_primary_key_is_read_only_even_when_rowid_aliases_are_available() {
            let mut table = test_support::table::minimal("", "");
            table.columns = vec![
                test_support::column::test_nullable_column("rowid", "TEXT", 1),
                test_support::column::test_nullable_column("_rowid_", "TEXT", 2),
                test_support::column::test_nullable_column("oid", "TEXT", 3),
            ];
            let state = sqlite_preview_state_with_table(table);

            assert!(!state.can_write_visible_preview());
            assert_eq!(
                state.visible_preview_target_read_only_reason(),
                Some("table without PRIMARY KEY")
            );
        }

        #[test]
        fn postgres_table_without_primary_key_is_read_only() {
            let state = postgres_preview_state_with_table(test_support::table::minimal("", ""));

            assert!(!state.can_write_visible_preview());
            assert_eq!(
                state.visible_preview_target_read_only_reason(),
                Some("table without PRIMARY KEY")
            );
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

                match status {
                    ErStatus::Idle => state.er_preparation.mark_idle(),
                    ErStatus::Waiting => {
                        let _ = state.er_preparation.start_waiting_run();
                    }
                    ErStatus::Rendering => state.er_preparation.mark_rendering(),
                }

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
                    .select_table("public", "users", &mut state.query);
                let generation = state.session.selection_generation();
                activate_postgres_connection(&mut state, "dsn://test");
                let run_id = state.session.begin_table_detail_run();
                state.ui.set_inspector_scroll_offset(42);

                dispatch_metadata(
                    &mut state,
                    &Action::TableDetailLoaded {
                        dsn: "dsn://test".to_string(),
                        run_id,
                        outcome: Ok(Box::new(make_table_detail())),
                        generation,
                    },
                    Instant::now(),
                );

                assert_eq!(state.ui.inspector_scroll_offset(), 0);
            }

            #[test]
            fn offset_defaults_to_zero() {
                let state = make_state();

                assert_eq!(state.ui.inspector_scroll_offset(), 0);
                assert!(state.session.table_detail().is_none());
            }
        }
    }

    mod connection_catalog {
        use super::*;
        use crate::domain::connection::SslMode;

        fn make_profile(name: &str) -> ConnectionProfile {
            ConnectionProfile::new_postgres(
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

        fn make_service(name: &str) -> ServiceEntry {
            ServiceEntry {
                service_name: name.to_string(),
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
