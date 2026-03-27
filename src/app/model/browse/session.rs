use std::sync::Arc;

use crate::app::model::browse::query_execution::{PaginationState, QueryExecution};
use crate::app::model::browse::result_history::ResultHistory;
use crate::app::model::connection::cache::ConnectionCache;
use crate::app::model::connection::state::ConnectionState;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::domain::{
    ConnectionId, DatabaseMetadata, MetadataState, QueryResult, Table, TableSummary,
};

// # Invariants
//
// - `connection_state` and `metadata_state` always transition as a pair
//   (e.g. `begin_connecting` sets both to Connecting/Loading).
// - `selected_table_key`, `table_detail`, and `selection_generation` change
//   together via `select_table` / `clear_table_selection`.
// - `database_name` is derived from `metadata` (single source of truth).
//
// # Transitional raw setters
//
// `set_metadata`, `set_table_detail_raw`, `set_connection_state`,
// `set_metadata_state` are `pub(crate)` for reducers where the aggregate API
// does not cover the exact semantics needed (e.g. ER refresh, reload).
#[derive(Debug, Clone, Default)]
pub struct BrowseSession {
    // -- co-dependent: connection lifecycle --
    connection_state: ConnectionState,
    metadata_state: MetadataState,

    // -- co-dependent: table selection --
    selected_table_key: Option<String>,
    table_detail: Option<Table>,
    selection_generation: u64,

    // -- lifecycle-gated --
    metadata: Option<Arc<DatabaseMetadata>>,

    // -- public / independent --
    pub dsn: Option<String>,
    pub active_connection_id: Option<ConnectionId>,
    pub active_connection_name: Option<String>,
    pub read_only: bool,
    pub is_reloading: bool,
}

impl BrowseSession {
    // ── Table selection ──────────────────────────────────────────────

    #[must_use]
    pub fn select_table(
        &mut self,
        schema: &str,
        table: &str,
        pagination: &mut PaginationState,
    ) -> u64 {
        self.selected_table_key = Some(format!("{schema}.{table}"));
        self.table_detail = None;
        self.selection_generation += 1;
        pagination.reset();
        pagination.schema = schema.to_string();
        pagination.table = table.to_string();
        self.selection_generation
    }

    #[must_use]
    pub fn set_table_detail(&mut self, detail: Table, generation: u64) -> bool {
        if generation == self.selection_generation {
            self.table_detail = Some(detail);
            true
        } else {
            false
        }
    }

    pub fn clear_table_selection(&mut self, pagination: &mut PaginationState) {
        self.selected_table_key = None;
        self.table_detail = None;
        self.selection_generation += 1;
        pagination.reset();
    }

    // ── Connection lifecycle ─────────────────────────────────────────

    pub fn begin_connecting(&mut self, dsn: &str) {
        self.dsn = Some(dsn.to_string());
        self.connection_state = ConnectionState::Connecting;
        self.metadata_state = MetadataState::Loading;
    }

    pub fn mark_connected(&mut self, metadata: Arc<DatabaseMetadata>) {
        self.connection_state = ConnectionState::Connected;
        self.metadata_state = MetadataState::Loaded;
        self.metadata = Some(metadata);
    }

    // On reload failure (already Connected), keeps Connected to preserve
    // the current browse session while surfacing the error.
    pub fn mark_connection_failed(&mut self, error: String) {
        self.metadata_state = MetadataState::Error(error);
        self.is_reloading = false;
        if !self.connection_state.is_connected() {
            self.connection_state = ConnectionState::Failed;
        }
    }

    pub fn begin_reload(&mut self) {
        self.is_reloading = true;
    }

    pub fn finish_reload(&mut self) {
        self.is_reloading = false;
    }

    // ── Cache operations ─────────────────────────────────────────────

    pub fn to_cache(
        &self,
        explorer_selected: usize,
        inspector_tab: InspectorTab,
        query_result: Option<Arc<QueryResult>>,
        result_history: ResultHistory,
    ) -> ConnectionCache {
        ConnectionCache {
            metadata: self.metadata.clone(),
            table_detail: self.table_detail.clone(),
            selected_table_key: self.selected_table_key.clone(),
            query_result,
            result_history,
            explorer_selected,
            inspector_tab,
        }
    }

    // Caller must also call `result_interaction.reset_view()` and restore UI state.
    pub fn restore_from_cache(&mut self, cache: &ConnectionCache, query: &mut QueryExecution) {
        self.metadata.clone_from(&cache.metadata);
        self.table_detail.clone_from(&cache.table_detail);
        self.selected_table_key
            .clone_from(&cache.selected_table_key);
        self.connection_state = ConnectionState::Connected;
        self.metadata_state = MetadataState::Loaded;
        self.selection_generation = 0;
        self.is_reloading = false;
        match &cache.query_result {
            Some(r) => query.set_current_result(r.clone()),
            None => query.clear_current_result(),
        }
        query.restore_history(cache.result_history.clone());
        query.exit_history();
    }

    // Caller must also call `result_interaction.reset_view()` and restore UI state.
    pub fn reset(&mut self, query: &mut QueryExecution) {
        self.metadata = None;
        self.table_detail = None;
        self.selected_table_key = None;
        self.selection_generation = 0;
        self.connection_state = ConnectionState::default();
        self.metadata_state = MetadataState::default();
        self.dsn = None;
        self.active_connection_id = None;
        self.active_connection_name = None;
        self.read_only = false;
        self.is_reloading = false;
        query.pagination.reset();
        query.clear_current_result();
        query.restore_history(ResultHistory::default());
        query.exit_history();
    }

    // ── Getters ──────────────────────────────────────────────────────

    pub fn connection_state(&self) -> ConnectionState {
        self.connection_state
    }

    pub fn metadata_state(&self) -> &MetadataState {
        &self.metadata_state
    }

    pub fn metadata(&self) -> Option<&Arc<DatabaseMetadata>> {
        self.metadata.as_ref()
    }

    pub fn database_name(&self) -> Option<&str> {
        self.metadata.as_ref().map(|m| m.database_name.as_str())
    }

    pub fn selected_table_key(&self) -> Option<&str> {
        self.selected_table_key.as_deref()
    }

    pub fn table_detail(&self) -> Option<&Table> {
        self.table_detail.as_ref()
    }

    pub fn selection_generation(&self) -> u64 {
        self.selection_generation
    }

    pub fn tables(&self) -> Vec<&TableSummary> {
        self.metadata
            .as_ref()
            .map(|m| m.table_summaries.iter().collect())
            .unwrap_or_default()
    }

    pub fn is_service_connection(&self) -> bool {
        self.dsn.as_ref().is_some_and(|d| d.starts_with("service="))
    }

    pub(crate) fn set_metadata_state(&mut self, state: MetadataState) {
        self.metadata_state = state;
    }

    pub(crate) fn set_connection_state(&mut self, state: ConnectionState) {
        self.connection_state = state;
    }

    pub(crate) fn set_metadata(&mut self, metadata: Option<Arc<DatabaseMetadata>>) {
        self.metadata = metadata;
    }

    pub(crate) fn set_table_detail_raw(&mut self, detail: Option<Table>) {
        self.table_detail = detail;
    }

    #[cfg(any(test, feature = "test-support"))]
    #[allow(dead_code, reason = "test-support helper")]
    pub(crate) fn set_selection_generation(&mut self, value: u64) {
        self.selection_generation = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DatabaseMetadata, QueryResult, QuerySource, TableSummary};
    use std::time::Instant;

    fn make_metadata(db_name: &str) -> Arc<DatabaseMetadata> {
        Arc::new(DatabaseMetadata {
            database_name: db_name.to_string(),
            schemas: vec![],
            table_summaries: vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ],
            fetched_at: Instant::now(),
        })
    }

    fn make_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: vec![],
            primary_key: None,
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: Some(100),
            comment: None,
        }
    }

    fn make_query_result() -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT 1".to_string(),
            vec!["col".to_string()],
            vec![vec!["val".to_string()]],
            10,
            QuerySource::Preview,
        ))
    }

    // ── select_table ─────────────────────────────────────────────────

    mod select_table {
        use super::*;

        #[test]
        fn increments_generation() {
            let mut session = BrowseSession::default();
            let mut pagination = PaginationState::default();

            let gen1 = session.select_table("public", "users", &mut pagination);
            let gen2 = session.select_table("public", "posts", &mut pagination);

            assert_eq!(gen1, 1);
            assert_eq!(gen2, 2);
        }

        #[test]
        fn clears_table_detail() {
            let mut session = BrowseSession::default();
            session.set_table_detail_raw(Some(make_table_detail()));
            let mut pagination = PaginationState::default();

            let _ = session.select_table("public", "users", &mut pagination);

            assert!(session.table_detail().is_none());
        }

        #[test]
        fn sets_selected_table_key() {
            let mut session = BrowseSession::default();
            let mut pagination = PaginationState::default();

            let _ = session.select_table("public", "users", &mut pagination);

            assert_eq!(session.selected_table_key(), Some("public.users"));
        }

        #[test]
        fn resets_pagination() {
            let mut session = BrowseSession::default();
            let mut pagination = PaginationState {
                current_page: 5,
                total_rows_estimate: Some(10000),
                reached_end: true,
                schema: "old".to_string(),
                table: "old".to_string(),
            };

            let _ = session.select_table("public", "users", &mut pagination);

            assert_eq!(pagination.current_page, 0);
            assert_eq!(pagination.total_rows_estimate, None);
            assert!(!pagination.reached_end);
            assert_eq!(pagination.schema, "public");
            assert_eq!(pagination.table, "users");
        }
    }

    // ── set_table_detail ─────────────────────────────────────────────

    mod set_table_detail_tests {
        use super::*;

        #[test]
        fn accepts_matching_generation() {
            let mut session = BrowseSession::default();
            let mut pagination = PaginationState::default();
            let generation = session.select_table("public", "users", &mut pagination);

            let accepted = session.set_table_detail(make_table_detail(), generation);

            assert!(accepted);
            assert!(session.table_detail().is_some());
        }

        #[test]
        fn rejects_stale_generation() {
            let mut session = BrowseSession::default();
            let mut pagination = PaginationState::default();
            let old_gen = session.select_table("public", "users", &mut pagination);
            let _ = session.select_table("public", "posts", &mut pagination);

            let accepted = session.set_table_detail(make_table_detail(), old_gen);

            assert!(!accepted);
            assert!(session.table_detail().is_none());
        }
    }

    // ── clear_table_selection ────────────────────────────────────────

    #[test]
    fn clear_table_selection_clears_all() {
        let mut session = BrowseSession::default();
        let mut pagination = PaginationState::default();
        let _ = session.select_table("public", "users", &mut pagination);
        let _ = session.set_table_detail(make_table_detail(), session.selection_generation());

        session.clear_table_selection(&mut pagination);

        assert!(session.selected_table_key().is_none());
        assert!(session.table_detail().is_none());
        assert_eq!(pagination.current_page, 0);
    }

    #[test]
    fn clear_table_selection_invalidates_pending_detail() {
        let mut session = BrowseSession::default();
        let mut pagination = PaginationState::default();
        let pre_clear_gen = session.select_table("public", "users", &mut pagination);

        session.clear_table_selection(&mut pagination);

        // A TableDetailLoaded arriving with the pre-clear generation must be rejected
        let accepted = session.set_table_detail(make_table_detail(), pre_clear_gen);
        assert!(!accepted);
        assert!(session.table_detail().is_none());
    }

    // ── Connection lifecycle ─────────────────────────────────────────

    mod connection_lifecycle {
        use super::*;

        #[test]
        fn begin_connecting_sets_pair() {
            let mut session = BrowseSession::default();

            session.begin_connecting("postgres://localhost/test");

            assert!(session.connection_state().is_connecting());
            assert_eq!(session.metadata_state(), &MetadataState::Loading);
            assert_eq!(session.dsn, Some("postgres://localhost/test".to_string()));
        }

        #[test]
        fn mark_connected_sets_pair_and_metadata() {
            let mut session = BrowseSession::default();
            let metadata = make_metadata("test_db");

            session.mark_connected(metadata);

            assert!(session.connection_state().is_connected());
            assert_eq!(session.metadata_state(), &MetadataState::Loaded);
            assert!(session.metadata().is_some());
            assert_eq!(session.database_name(), Some("test_db"));
        }

        #[test]
        fn mark_connection_failed_when_not_connected() {
            let mut session = BrowseSession::default();
            session.set_connection_state(ConnectionState::Connecting);

            session.mark_connection_failed("timeout".to_string());

            assert!(session.connection_state().is_failed());
            assert_eq!(
                session.metadata_state(),
                &MetadataState::Error("timeout".to_string())
            );
            assert!(!session.is_reloading);
        }

        #[test]
        fn mark_connection_failed_when_connected_keeps_connected() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            session.is_reloading = true;

            session.mark_connection_failed("reload timeout".to_string());

            assert!(session.connection_state().is_connected());
            assert_eq!(
                session.metadata_state(),
                &MetadataState::Error("reload timeout".to_string())
            );
            assert!(!session.is_reloading);
        }

        #[test]
        fn begin_reload_and_finish_reload() {
            let mut session = BrowseSession::default();

            session.begin_reload();
            assert!(session.is_reloading);

            session.finish_reload();
            assert!(!session.is_reloading);
        }
    }

    // ── to_cache / restore_from_cache round-trip ─────────────────────

    mod cache_round_trip {
        use super::*;

        #[test]
        fn round_trip_preserves_state() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("round_trip_db"));
            let mut pagination = PaginationState::default();
            let _ = session.select_table("public", "users", &mut pagination);
            let _ = session.set_table_detail(make_table_detail(), session.selection_generation());

            let result = make_query_result();
            let mut history = ResultHistory::default();
            history.push(result.clone());

            let cache = session.to_cache(5, InspectorTab::Indexes, Some(result), history);

            // Create a fresh session and restore
            let mut new_session = BrowseSession::default();
            let mut query = QueryExecution::default();
            new_session.restore_from_cache(&cache, &mut query);

            assert_eq!(new_session.database_name(), Some("round_trip_db"));
            assert!(new_session.table_detail().is_some());
            assert_eq!(new_session.selected_table_key(), Some("public.users"));
            assert!(new_session.connection_state().is_connected());
            assert_eq!(new_session.metadata_state(), &MetadataState::Loaded);
            assert!(query.current_result().is_some());
            assert_eq!(query.result_history().len(), 1);
            assert!(query.history_index().is_none());
        }

        #[test]
        fn restore_resets_generation_and_reloading() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let mut pagination = PaginationState::default();
            let _ = session.select_table("public", "users", &mut pagination);
            session.is_reloading = true;
            assert!(session.selection_generation() > 0);

            let cache = session.to_cache(0, InspectorTab::Info, None, ResultHistory::default());

            let mut new_session = BrowseSession::default();
            new_session.set_selection_generation(42);
            new_session.is_reloading = true;
            let mut query = QueryExecution::default();
            new_session.restore_from_cache(&cache, &mut query);

            assert_eq!(new_session.selection_generation(), 0);
            assert!(!new_session.is_reloading);
        }

        #[test]
        fn restore_then_begin_reload_preserves_selection() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let mut pagination = PaginationState::default();
            let generation = session.select_table("public", "users", &mut pagination);
            let _ = session.set_table_detail(make_table_detail(), generation);

            let cache = session.to_cache(
                3,
                InspectorTab::Columns,
                Some(make_query_result()),
                ResultHistory::default(),
            );

            let mut restored = BrowseSession::default();
            let mut query = QueryExecution::default();
            restored.restore_from_cache(&cache, &mut query);
            restored.begin_reload();

            assert_eq!(restored.selected_table_key(), Some("public.users"));
            assert!(restored.table_detail().is_some());
            assert!(restored.is_reloading);
            assert!(restored.connection_state().is_connected());
        }
    }

    // ── reset ────────────────────────────────────────────────────────

    mod reset_tests {
        use super::*;

        #[test]
        fn reset_clears_everything() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            session.dsn = Some("postgres://host/db".to_string());
            session.active_connection_id = Some(ConnectionId::new());
            session.active_connection_name = Some("mydb".to_string());
            session.read_only = true;
            session.is_reloading = true;
            let mut query = QueryExecution::default();
            query.set_current_result(make_query_result());
            query.pagination = PaginationState {
                current_page: 3,
                total_rows_estimate: Some(1000),
                reached_end: true,
                schema: "public".to_string(),
                table: "users".to_string(),
            };
            query.enter_history(2);

            session.reset(&mut query);

            assert!(session.connection_state().is_not_connected());
            assert_eq!(session.metadata_state(), &MetadataState::NotLoaded);
            assert!(session.metadata().is_none());
            assert!(session.database_name().is_none());
            assert!(session.selected_table_key().is_none());
            assert!(session.table_detail().is_none());
            assert_eq!(session.selection_generation(), 0);
            assert!(session.dsn.is_none());
            assert!(session.active_connection_id.is_none());
            assert!(session.active_connection_name.is_none());
            assert!(!session.read_only);
            assert!(!session.is_reloading);
            assert_eq!(query.pagination.current_page, 0);
            assert!(query.current_result().is_none());
            assert!(query.history_index().is_none());
        }
    }

    // ── database_name derived from metadata ──────────────────────────

    mod database_name_tests {
        use super::*;

        #[test]
        fn none_when_no_metadata() {
            let session = BrowseSession::default();
            assert!(session.database_name().is_none());
        }

        #[test]
        fn returns_name_after_mark_connected() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("mydb"));
            assert_eq!(session.database_name(), Some("mydb"));
        }

        #[test]
        fn cleared_after_reset() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("mydb"));
            let mut query = QueryExecution::default();
            session.reset(&mut query);
            assert!(session.database_name().is_none());
        }

        #[test]
        fn synced_after_restore_from_cache() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("cached_db"));

            let cache = session.to_cache(0, InspectorTab::Info, None, ResultHistory::default());

            let mut new_session = BrowseSession::default();
            let mut query = QueryExecution::default();
            new_session.restore_from_cache(&cache, &mut query);

            assert_eq!(new_session.database_name(), Some("cached_db"));
        }

        #[test]
        fn none_when_cache_has_no_metadata() {
            let cache = ConnectionCache::default();
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            session.restore_from_cache(&cache, &mut query);

            assert!(session.database_name().is_none());
        }
    }

    // ── Getters ──────────────────────────────────────────────────────

    mod getter_tests {
        use super::*;

        #[test]
        fn tables_returns_empty_when_no_metadata() {
            let session = BrowseSession::default();
            assert!(session.tables().is_empty());
        }

        #[test]
        fn tables_returns_all_when_metadata_present() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            assert_eq!(session.tables().len(), 2);
        }

        #[test]
        fn is_service_connection_detects_service_dsn() {
            let session = BrowseSession {
                dsn: Some("service=myservice".to_string()),
                ..Default::default()
            };
            assert!(session.is_service_connection());
        }

        #[test]
        fn is_service_connection_false_for_normal_dsn() {
            let session = BrowseSession {
                dsn: Some("postgres://localhost/db".to_string()),
                ..Default::default()
            };
            assert!(!session.is_service_connection());
        }

        #[test]
        fn default_state() {
            let session = BrowseSession::default();
            assert!(session.connection_state().is_not_connected());
            assert_eq!(session.metadata_state(), &MetadataState::NotLoaded);
            assert!(session.metadata().is_none());
            assert!(session.selected_table_key().is_none());
            assert!(session.table_detail().is_none());
            assert_eq!(session.selection_generation(), 0);
        }
    }
}
