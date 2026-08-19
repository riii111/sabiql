use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;

use crate::domain::query_history::QueryHistoryScope;
use crate::domain::{
    ConnectionId, DatabaseMetadata, DatabaseType, MetadataState, QueryResult, Table, TableSummary,
};
use crate::model::browse::query_execution::{PaginationState, QueryExecution};
use crate::model::browse::result_history::ResultHistory;
use crate::model::connection::cache::ConnectionCache;
use crate::model::connection::origin::ConnectionOrigin;
use crate::model::connection::state::ConnectionState;
use crate::model::shared::async_run::AsyncRun;
use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::policy::mask_password;

#[derive(Debug, Default)]
pub struct ConnectionSaveGuard {
    state: Mutex<ConnectionSaveState>,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum ConnectionSaveState {
    #[default]
    Idle,
    Active(u64),
    Claimed(u64),
    Saving(u64),
}

impl ConnectionSaveGuard {
    pub(crate) fn start(&self, run_id: u64) {
        *self.state.lock().expect("connection save guard poisoned") =
            ConnectionSaveState::Active(run_id);
    }

    pub(crate) fn cancel(&self) {
        *self.state.lock().expect("connection save guard poisoned") = ConnectionSaveState::Idle;
    }

    pub(crate) fn claim(&self, run_id: u64) -> bool {
        let mut state = self.state.lock().expect("connection save guard poisoned");
        if *state != ConnectionSaveState::Active(run_id) {
            return false;
        }
        *state = ConnectionSaveState::Claimed(run_id);
        true
    }

    pub(crate) fn start_save(&self, run_id: u64) -> bool {
        let mut state = self.state.lock().expect("connection save guard poisoned");
        if *state != ConnectionSaveState::Claimed(run_id) {
            return false;
        }
        *state = ConnectionSaveState::Saving(run_id);
        true
    }

    pub(crate) fn finish_save(&self, run_id: u64) {
        let mut state = self.state.lock().expect("connection save guard poisoned");
        if *state == ConnectionSaveState::Saving(run_id) {
            *state = ConnectionSaveState::Idle;
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveConnection {
    id: ConnectionId,
    name: String,
    database_type: DatabaseType,
    origin: ConnectionOrigin,
    database: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TableDetailState {
    NotSelected,
    Loading,
    Loaded(Box<Table>),
    Error(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct PendingMySqlConnectionProbe {
    pub id: ConnectionId,
    pub name: String,
    pub database_type: DatabaseType,
    pub dsn: String,
    pub database: Option<String>,
    pub run_id: u64,
    pub table_detail_dsn: Option<String>,
    pub table_detail_run_id: Option<u64>,
    pub table_detail_generation: u64,
}

impl fmt::Debug for PendingMySqlConnectionProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingMySqlConnectionProbe")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("database_type", &self.database_type)
            .field("dsn", &mask_password(&self.dsn))
            .field("database", &self.database)
            .field("run_id", &self.run_id)
            .field(
                "table_detail_dsn",
                &self.table_detail_dsn.as_deref().map(mask_password),
            )
            .field("table_detail_run_id", &self.table_detail_run_id)
            .field("table_detail_generation", &self.table_detail_generation)
            .finish()
    }
}

// # Invariants
//
// - `connection_state` and `metadata_state` always transition as a pair
//   (e.g. `begin_connecting` sets both to Connecting/Loading).
// - `selected_table_key`, `table_detail_state`, and `selection_generation`
//   change together via `select_table` / `clear_table_selection`.
// - `database_name` is derived from `metadata` (single source of truth).
// - Cache restore for a connection exits transient reload/read-only state.
//
// # Transitional raw setters
//
// `set_metadata` and `set_table_detail_raw` are `pub(crate)` for reducers
// where the aggregate API does not cover the exact semantics needed.
// `set_connection_state` and `set_metadata_state` are test-only lifecycle
// fixtures.
#[derive(Debug, Clone)]
pub struct BrowseSession {
    // -- co-dependent: connection lifecycle --
    connection_state: ConnectionState,
    metadata_state: MetadataState,

    // -- co-dependent: table selection --
    selected_table_key: Option<String>,
    table_detail_state: TableDetailState,
    selection_generation: u64,

    // -- lifecycle-gated --
    metadata: Option<Arc<DatabaseMetadata>>,
    metadata_run: AsyncRun,
    effective_user: Option<String>,
    effective_user_run: AsyncRun,
    table_detail_run: AsyncRun,
    connection_save_run: AsyncRun,
    connection_save_guard: Arc<ConnectionSaveGuard>,

    // -- co-dependent: connection identity / lifecycle --
    dsn: Option<String>,
    active_connection: Option<ActiveConnection>,
    mysql_connection_probe_run: AsyncRun,
    pending_mysql_connection_probe: Option<PendingMySqlConnectionProbe>,
    connection_generation: u64,
    database_generation: u64,
    active_engine_feature_profile: EngineFeatureProfile,
    read_only: bool,
    is_reloading: bool,
}

impl Default for BrowseSession {
    fn default() -> Self {
        Self {
            connection_state: ConnectionState::default(),
            metadata_state: MetadataState::default(),
            selected_table_key: None,
            table_detail_state: TableDetailState::NotSelected,
            selection_generation: 0,
            metadata: None,
            metadata_run: AsyncRun::default(),
            effective_user: None,
            effective_user_run: AsyncRun::default(),
            table_detail_run: AsyncRun::default(),
            connection_save_run: AsyncRun::default(),
            connection_save_guard: Arc::new(ConnectionSaveGuard::default()),
            dsn: None,
            active_connection: None,
            mysql_connection_probe_run: AsyncRun::default(),
            pending_mysql_connection_probe: None,
            connection_generation: 0,
            database_generation: 0,
            active_engine_feature_profile: EngineFeatureProfile::disconnected(),
            read_only: false,
            is_reloading: false,
        }
    }
}

impl BrowseSession {
    // ── Table selection ──────────────────────────────────────────────

    #[must_use]
    pub fn select_table(&mut self, schema: &str, table: &str, query: &mut QueryExecution) -> u64 {
        query.reset_for_context_change();
        query.clear_current_result();
        self.selected_table_key = Some(format!("{schema}.{table}"));
        self.table_detail_state = TableDetailState::Loading;
        self.selection_generation += 1;
        self.table_detail_run.clear_active();
        query.pagination.reset_for_table(schema, table);
        self.selection_generation
    }

    #[must_use]
    pub fn set_table_detail(&mut self, detail: Table, generation: u64) -> bool {
        if generation == self.selection_generation {
            self.table_detail_state = TableDetailState::Loaded(Box::new(detail));
            true
        } else {
            false
        }
    }

    pub fn clear_table_selection(&mut self, query: &mut QueryExecution) {
        query.reset_for_context_change();
        query.clear_current_result();
        self.selected_table_key = None;
        self.table_detail_state = TableDetailState::NotSelected;
        self.selection_generation += 1;
        self.table_detail_run.clear_active();
        query.pagination.reset();
    }

    #[must_use]
    pub fn begin_table_detail_run(&mut self) -> u64 {
        if self.selected_table_key.is_some() {
            self.table_detail_state = TableDetailState::Loading;
        }
        self.table_detail_run.begin()
    }

    pub fn is_current_table_detail_run(&self, run_id: u64) -> bool {
        self.table_detail_run.is_current(run_id)
    }

    pub fn is_current_table_selection(&self, schema: &str, table: &str, generation: u64) -> bool {
        generation == self.selection_generation
            && self.selected_table_key.as_deref() == Some(&format!("{schema}.{table}"))
    }

    pub fn mark_table_detail_failed(&mut self, generation: u64, error: String) -> bool {
        if generation == self.selection_generation && self.selected_table_key.is_some() {
            self.table_detail_state = TableDetailState::Error(error);
            true
        } else {
            false
        }
    }

    pub fn mark_table_detail_probe_failed(&mut self, dsn: &str, error: String) -> bool {
        let belongs_to_probe =
            self.pending_mysql_connection_probe
                .as_ref()
                .is_some_and(|pending| {
                    pending.table_detail_dsn.as_deref() == Some(dsn)
                        && pending.table_detail_run_id == Some(self.table_detail_run.last_id())
                        && pending.table_detail_generation == self.selection_generation
                });
        if belongs_to_probe
            && self.dsn_matches(dsn)
            && self.selected_table_key.is_some()
            && matches!(&self.table_detail_state, TableDetailState::Loading)
        {
            self.table_detail_state = TableDetailState::Error(error);
            true
        } else {
            false
        }
    }

    pub fn retry_table_detail_after_probe_failure(&mut self) -> Option<(String, u64, u64)> {
        let pending = self.pending_mysql_connection_probe.as_ref()?;
        let dsn = pending.table_detail_dsn.clone()?;
        let generation = pending.table_detail_generation;
        let run_id = pending.table_detail_run_id?;
        if run_id != self.table_detail_run.last_id()
            || generation != self.selection_generation
            || !self.dsn_matches(&dsn)
            || self.selected_table_key.is_none()
            || !matches!(&self.table_detail_state, TableDetailState::Loading)
        {
            return None;
        }

        let retry_run_id = self.begin_table_detail_run();
        Some((dsn, generation, retry_run_id))
    }

    // ── Connection lifecycle ─────────────────────────────────────────

    pub fn mark_connecting(&mut self) {
        self.connection_state = ConnectionState::Connecting;
        self.metadata_state = MetadataState::Loading;
        self.effective_user = None;
        self.effective_user_run.clear_active();
    }

    #[must_use]
    pub fn begin_connection_save(&mut self) -> u64 {
        let run_id = self.connection_save_run.begin();
        self.connection_save_guard.start(run_id);
        run_id
    }

    pub fn is_current_connection_save(&self, run_id: u64) -> bool {
        self.connection_save_run.is_current(run_id)
    }

    pub fn cancel_connection_save(&mut self) {
        self.connection_save_guard.cancel();
        self.connection_save_run.clear_active();
    }

    pub fn cancel_connection_save_and_disconnect(&mut self) {
        if self.connection_save_run.active_id().is_some() {
            self.cancel_connection_save();
            self.mark_disconnected();
        }
    }

    pub fn connection_save_guard(&self) -> Arc<ConnectionSaveGuard> {
        Arc::clone(&self.connection_save_guard)
    }

    #[must_use]
    pub fn begin_mysql_connection_probe(
        &mut self,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
        dsn: &str,
        database: Option<&str>,
    ) -> u64 {
        let table_detail_dsn = self.dsn.clone();
        let table_detail_run_id = self.table_detail_run.active_id();
        let table_detail_generation = self.selection_generation;
        self.cancel_metadata_for_mysql_connection_probe();
        self.connection_generation = self.connection_generation.wrapping_add(1);
        let run_id = self.mysql_connection_probe_run.begin();
        self.pending_mysql_connection_probe = Some(PendingMySqlConnectionProbe {
            id: id.clone(),
            name: name.to_string(),
            database_type,
            dsn: dsn.to_string(),
            database: database.map(str::to_string),
            run_id,
            table_detail_dsn,
            table_detail_run_id,
            table_detail_generation,
        });
        run_id
    }

    pub fn invalidate_connection_generation(&mut self) {
        self.connection_generation = self.connection_generation.wrapping_add(1);
    }

    fn cancel_metadata_for_mysql_connection_probe(&mut self) {
        self.metadata_run.clear_active();
        self.effective_user_run.clear_active();
        self.table_detail_run.clear_active();
        self.is_reloading = false;
        match self.connection_state {
            ConnectionState::Connecting => {
                self.connection_state = ConnectionState::NotConnected;
                self.metadata_state = MetadataState::NotLoaded;
            }
            ConnectionState::Connected => {
                self.metadata_state = if self.metadata.is_some() {
                    MetadataState::Loaded
                } else {
                    MetadataState::NotLoaded
                };
            }
            ConnectionState::Failed | ConnectionState::NotConnected => {}
        }
    }

    pub fn is_current_mysql_connection_probe(
        &self,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
        dsn: &str,
        database: Option<&str>,
        run_id: u64,
    ) -> bool {
        self.mysql_connection_probe_run.is_current(run_id)
            && self
                .pending_mysql_connection_probe
                .as_ref()
                .is_some_and(|pending| {
                    pending.run_id == run_id
                        && pending.id == *id
                        && pending.name == name
                        && pending.database_type == database_type
                        && pending.dsn == dsn
                        && pending.database.as_deref() == database
                })
    }

    pub fn pending_mysql_connection_probe(&self) -> Option<&PendingMySqlConnectionProbe> {
        self.pending_mysql_connection_probe.as_ref()
    }

    pub fn has_pending_connection_switch(&self) -> bool {
        let Some(pending) = self.pending_mysql_connection_probe.as_ref() else {
            return false;
        };

        self.active_connection_id()
            .is_some_and(|id| id != &pending.id)
            || self.dsn().is_some_and(|dsn| dsn != pending.dsn)
    }

    pub fn clear_mysql_connection_probe(&mut self) {
        self.mysql_connection_probe_run.clear_active();
        self.pending_mysql_connection_probe = None;
    }

    #[must_use]
    pub fn begin_connecting(&mut self, dsn: &str) -> u64 {
        self.clear_mysql_connection_probe();
        self.dsn = Some(dsn.to_string());
        self.mark_connecting();
        self.begin_metadata_run()
    }

    pub fn activate_connection_with_dsn(
        &mut self,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
        dsn: &str,
    ) {
        self.activate_connection_with_target(id, name, database_type, dsn, None);
    }

    pub fn activate_connection_with_target(
        &mut self,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
        dsn: &str,
        database: Option<&str>,
    ) {
        self.database_generation = self.database_generation.wrapping_add(1);
        self.active_connection = Some(ActiveConnection {
            id: id.clone(),
            name: name.to_string(),
            database_type,
            origin: ConnectionOrigin::Profile,
            database: database.map(str::to_string),
        });
        self.active_engine_feature_profile = EngineFeatureProfile::for_database_type(database_type);
        self.dsn = Some(dsn.to_string());
        self.read_only = false;
        self.clear_mysql_connection_probe();
    }

    pub fn activate_cli_ephemeral_connection(&mut self, id: &ConnectionId, name: &str, dsn: &str) {
        self.active_connection = Some(ActiveConnection {
            id: id.clone(),
            name: name.to_string(),
            database_type: DatabaseType::SQLite,
            origin: ConnectionOrigin::CliEphemeral,
            database: None,
        });
        self.active_engine_feature_profile =
            EngineFeatureProfile::for_database_type(DatabaseType::SQLite);
        self.dsn = Some(dsn.to_string());
        self.read_only = false;
        self.clear_mysql_connection_probe();
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_active_connection_identity_for_test(
        &mut self,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
    ) {
        self.active_connection = Some(ActiveConnection {
            id: id.clone(),
            name: name.to_string(),
            database_type,
            origin: ConnectionOrigin::Profile,
            database: None,
        });
        self.active_engine_feature_profile = EngineFeatureProfile::for_database_type(database_type);
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_active_engine_feature_profile_for_test(&mut self, database_type: DatabaseType) {
        self.active_engine_feature_profile = EngineFeatureProfile::for_database_type(database_type);
    }

    pub fn clear_connection(&mut self) {
        self.dsn = None;
        self.active_connection = None;
        self.cancel_connection_save_and_disconnect();
        self.clear_mysql_connection_probe();
        self.active_engine_feature_profile = EngineFeatureProfile::disconnected();
    }

    pub fn mark_connected(&mut self, metadata: Arc<DatabaseMetadata>) {
        self.connection_state = ConnectionState::Connected;
        self.metadata_state = MetadataState::Loaded;
        self.metadata = Some(metadata);
        self.metadata_run.clear_active();
        self.effective_user = None;
        self.effective_user_run.clear_active();
    }

    pub fn mark_probe_connected(&mut self) {
        self.connection_state = ConnectionState::Connected;
        self.metadata_state = MetadataState::NotLoaded;
        self.metadata = None;
        self.metadata_run.clear_active();
        self.effective_user = None;
        self.effective_user_run.clear_active();
    }

    // On reload failure (already Connected), keeps Connected to preserve
    // the current browse session while surfacing the error.
    pub fn mark_connection_failed(&mut self, error: String) {
        self.metadata_state = MetadataState::Error(error);
        self.is_reloading = false;
        self.metadata_run.clear_active();
        if !self.connection_state.is_connected() {
            self.effective_user = None;
            self.effective_user_run.clear_active();
            self.connection_state = ConnectionState::Failed;
        }
    }

    #[must_use]
    pub fn begin_metadata_refresh(&mut self) -> u64 {
        self.clear_mysql_connection_probe();
        self.metadata_state = MetadataState::Loading;
        self.begin_metadata_run()
    }

    pub fn mark_disconnected(&mut self) {
        self.connection_state = ConnectionState::NotConnected;
        self.metadata_state = MetadataState::NotLoaded;
        self.is_reloading = false;
        self.metadata_run.clear_active();
        self.effective_user = None;
        self.effective_user_run.clear_active();
        self.table_detail_run.clear_active();
        self.clear_mysql_connection_probe();
    }

    #[must_use]
    pub fn begin_reload(&mut self) -> u64 {
        self.clear_mysql_connection_probe();
        self.is_reloading = true;
        self.begin_metadata_run()
    }

    pub fn finish_reload(&mut self) {
        self.is_reloading = false;
    }

    pub fn enable_read_only(&mut self) {
        self.read_only = true;
    }

    pub fn disable_read_only(&mut self) {
        self.read_only = false;
    }

    #[must_use]
    fn begin_metadata_run(&mut self) -> u64 {
        self.metadata_run.begin()
    }

    pub fn is_current_metadata_run(&self, run_id: u64) -> bool {
        self.metadata_run.is_current(run_id)
    }

    pub fn metadata_generation(&self) -> u64 {
        self.metadata_run.last_id()
    }

    pub fn is_current_completion_scope(
        &self,
        dsn: Option<&str>,
        connection_generation: u64,
        database_generation: u64,
        metadata_generation: u64,
    ) -> bool {
        self.dsn() == dsn
            && self.connection_generation == connection_generation
            && self.database_generation == database_generation
            && self.metadata_generation() == metadata_generation
    }

    #[must_use]
    pub fn begin_effective_user_fetch(&mut self) -> u64 {
        self.effective_user_run.begin()
    }

    pub fn is_current_effective_user_run(&self, run_id: u64) -> bool {
        self.effective_user_run.is_current(run_id)
    }

    pub fn mark_effective_user_loaded(&mut self, effective_user: Option<String>) {
        self.effective_user = effective_user;
        self.effective_user_run.clear_active();
    }

    // ── Cache operations ─────────────────────────────────────────────

    pub fn to_cache(
        &self,
        explorer_selected: usize,
        inspector_tab: InspectorTab,
        query_result: Option<Arc<QueryResult>>,
        result_history: ResultHistory,
        pagination: PaginationState,
    ) -> ConnectionCache {
        ConnectionCache {
            connection_dsn: self.dsn.clone(),
            database_type: self.active_database_type(),
            database: self.active_database().map(str::to_string),
            metadata: self.metadata.clone(),
            effective_user: self.effective_user.clone(),
            table_detail: self.table_detail().cloned(),
            selected_table_key: self.selected_table_key.clone(),
            query_result,
            result_history,
            pagination,
            explorer_selected,
            inspector_tab,
        }
    }

    fn restore_from_cache(&mut self, cache: &ConnectionCache, query: &mut QueryExecution) {
        query.reset_for_context_change();
        query.restore_pagination(cache.pagination.clone());
        self.metadata.clone_from(&cache.metadata);
        self.effective_user.clone_from(&cache.effective_user);
        self.selected_table_key
            .clone_from(&cache.selected_table_key);
        self.table_detail_state = match (&self.selected_table_key, &cache.table_detail) {
            (Some(_), Some(detail)) => TableDetailState::Loaded(Box::new(detail.clone())),
            (Some(_), None) | (None, _) => TableDetailState::NotSelected,
        };
        self.connection_state = ConnectionState::Connected;
        self.metadata_state = MetadataState::Loaded;
        self.selection_generation = 0;
        self.is_reloading = false;
        self.metadata_run.clear_active();
        self.effective_user_run.clear_active();
        self.table_detail_run.clear_active();
        self.clear_mysql_connection_probe();
        match &cache.query_result {
            Some(r) => query.set_current_result(r.clone()),
            None => query.clear_current_result(),
        }
        query.restore_history(cache.result_history.clone());
    }

    pub fn restore_from_cache_for_connection(
        &mut self,
        cache: &ConnectionCache,
        query: &mut QueryExecution,
        id: &ConnectionId,
        name: &str,
        database_type: DatabaseType,
        dsn: &str,
        database: Option<&str>,
    ) {
        self.restore_from_cache(cache, query);
        self.activate_connection_with_target(id, name, database_type, dsn, database);
    }

    // Caller must also call `result_interaction.reset_view()` and restore UI state.
    pub fn reset(&mut self, query: &mut QueryExecution) {
        query.reset_for_context_change();
        self.metadata = None;
        self.table_detail_state = TableDetailState::NotSelected;
        self.selected_table_key = None;
        self.selection_generation = 0;
        self.connection_state = ConnectionState::default();
        self.metadata_state = MetadataState::default();
        self.metadata_run.clear_active();
        self.effective_user = None;
        self.effective_user_run.clear_active();
        self.table_detail_run.clear_active();
        self.clear_connection();
        self.read_only = false;
        self.is_reloading = false;
        query.pagination.reset();
        query.clear_current_result();
        query.restore_history(ResultHistory::default());
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
        self.active_database()
            .or_else(|| self.metadata.as_ref().map(|m| m.database_name.as_str()))
    }

    pub fn effective_user(&self) -> Option<&str> {
        self.effective_user.as_deref()
    }

    pub fn selected_table_key(&self) -> Option<&str> {
        self.selected_table_key.as_deref()
    }

    pub fn table_detail(&self) -> Option<&Table> {
        match &self.table_detail_state {
            TableDetailState::Loaded(table) => Some(table),
            TableDetailState::NotSelected
            | TableDetailState::Loading
            | TableDetailState::Error(_) => None,
        }
    }

    pub fn table_detail_state(&self) -> &TableDetailState {
        &self.table_detail_state
    }

    pub(crate) fn is_table_detail_terminal(&self, generation: u64) -> bool {
        generation == self.selection_generation
            && matches!(
                self.table_detail_state,
                TableDetailState::Loaded(_) | TableDetailState::Error(_)
            )
    }

    pub fn selection_generation(&self) -> u64 {
        self.selection_generation
    }

    pub fn dsn(&self) -> Option<&str> {
        self.dsn.as_deref()
    }

    // Async completion reducers use this guard with run ids to reject stale
    // effects from a previous connection.
    pub fn dsn_matches(&self, expected: &str) -> bool {
        self.dsn() == Some(expected)
    }

    pub fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    pub fn database_generation(&self) -> u64 {
        self.database_generation
    }

    pub fn active_connection_id(&self) -> Option<&ConnectionId> {
        self.active_connection
            .as_ref()
            .map(|connection| &connection.id)
    }

    pub fn active_connection_name(&self) -> Option<&str> {
        self.active_connection
            .as_ref()
            .map(|connection| connection.name.as_str())
    }

    pub fn active_database_type(&self) -> Option<DatabaseType> {
        self.active_connection
            .as_ref()
            .map(|connection| connection.database_type)
    }

    pub fn active_database_type_or_default(&self) -> DatabaseType {
        self.active_database_type().unwrap_or_default()
    }

    pub fn active_database(&self) -> Option<&str> {
        self.active_connection
            .as_ref()
            .and_then(|connection| connection.database.as_deref())
    }

    pub fn query_history_scope(&self) -> Option<QueryHistoryScope> {
        let connection_id = self.active_connection_id()?.clone();
        let database = match self.active_database_type() {
            Some(DatabaseType::MySQL) => self.active_database().map(str::to_owned),
            Some(DatabaseType::PostgreSQL | DatabaseType::SQLite) | None => None,
        };
        Some(QueryHistoryScope::new(connection_id, database))
    }

    pub fn active_engine_feature_profile(&self) -> &EngineFeatureProfile {
        &self.active_engine_feature_profile
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn is_reloading(&self) -> bool {
        self.is_reloading
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

    pub fn is_ephemeral_connection(&self) -> bool {
        self.active_connection
            .as_ref()
            .is_some_and(|connection| connection.origin.is_ephemeral())
    }

    pub fn can_reenter_connection_setup(&self) -> bool {
        !self.is_service_connection() && !self.is_ephemeral_connection()
    }

    #[cfg(test)]
    pub(crate) fn set_metadata_state(&mut self, state: MetadataState) {
        self.metadata_state = state;
    }

    #[cfg(test)]
    pub(crate) fn set_connection_state(&mut self, state: ConnectionState) {
        self.connection_state = state;
    }

    pub(crate) fn set_metadata(&mut self, metadata: Option<Arc<DatabaseMetadata>>) {
        self.metadata = metadata;
    }

    pub(crate) fn set_table_detail_raw(&mut self, detail: Option<Table>) {
        self.table_detail_state = match detail {
            Some(detail) => TableDetailState::Loaded(Box::new(detail)),
            None if self.selected_table_key.is_some() => TableDetailState::Loading,
            None => TableDetailState::NotSelected,
        };
    }

    #[cfg(test)]
    pub(crate) fn set_selection_generation(&mut self, value: u64) {
        self.selection_generation = value;
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support;

    use super::*;
    use crate::domain::QuerySource;

    fn make_metadata(db_name: &str) -> Arc<DatabaseMetadata> {
        Arc::new({
            let mut metadata = DatabaseMetadata::new(db_name.to_string());
            metadata.table_summaries = vec![
                TableSummary::new("public".to_string(), "users".to_string(), Some(100), false),
                TableSummary::new("public".to_string(), "posts".to_string(), Some(50), false),
            ];
            metadata
        })
    }

    fn make_table_detail() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            row_count_estimate: Some(100),
            ..test_support::table::minimal("", "")
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
            let mut query = QueryExecution::default();

            let gen1 = session.select_table("public", "users", &mut query);
            let gen2 = session.select_table("public", "posts", &mut query);

            assert_eq!(gen1, 1);
            assert_eq!(gen2, 2);
        }

        #[test]
        fn clears_table_detail() {
            let mut session = BrowseSession::default();
            session.set_table_detail_raw(Some(make_table_detail()));
            let mut query = QueryExecution::default();

            let _ = session.select_table("public", "users", &mut query);

            assert!(session.table_detail().is_none());
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Loading
            ));
        }

        #[test]
        fn sets_selected_table_key() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();

            let _ = session.select_table("public", "users", &mut query);

            assert_eq!(session.selected_table_key(), Some("public.users"));
        }

        #[test]
        fn resets_pagination() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            query.pagination.reset_for_table("old", "old");
            query.pagination.set_total_rows_estimate(Some(10000));
            query.pagination.set_page_result(5, true);

            let _ = session.select_table("public", "users", &mut query);

            assert_eq!(query.pagination.current_page(), 0);
            assert_eq!(query.pagination.total_rows_estimate(), None);
            assert!(!query.pagination.reached_end());
            assert_eq!(query.pagination.schema(), "public");
            assert_eq!(query.pagination.table(), "users");
        }

        #[test]
        fn terminates_active_query_run() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let run_id = query.begin_running(std::time::Instant::now());

            let _ = session.select_table("public", "users", &mut query);

            assert!(!query.is_running());
            assert!(!query.is_current_run(run_id));
        }

        #[test]
        fn clears_previous_query_result() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            query.set_current_result(make_query_result());

            let _ = session.select_table("public", "users", &mut query);

            assert!(query.current_result().is_none());
        }
    }

    // ── set_table_detail ─────────────────────────────────────────────

    mod set_table_detail_tests {
        use super::*;

        #[test]
        fn accepts_matching_generation() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let generation = session.select_table("public", "users", &mut query);

            let accepted = session.set_table_detail(make_table_detail(), generation);

            assert!(accepted);
            assert!(session.table_detail().is_some());
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Loaded(_)
            ));
        }

        #[test]
        fn rejects_stale_generation() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let old_gen = session.select_table("public", "users", &mut query);
            let _ = session.select_table("public", "posts", &mut query);

            let accepted = session.set_table_detail(make_table_detail(), old_gen);

            assert!(!accepted);
            assert!(session.table_detail().is_none());
        }

        #[test]
        fn records_error_for_current_generation() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let generation = session.select_table("public", "users", &mut query);

            assert!(session.mark_table_detail_failed(generation, "boom".to_string()));
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Error(error) if error == "boom"
            ));
        }

        #[test]
        fn rejects_error_for_stale_generation() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let old_generation = session.select_table("public", "users", &mut query);
            let _ = session.select_table("public", "posts", &mut query);

            assert!(!session.mark_table_detail_failed(old_generation, "boom".to_string()));
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Loading
            ));
        }

        #[test]
        fn starting_a_new_run_clears_previous_success() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let generation = session.select_table("public", "users", &mut query);
            let _ = session.set_table_detail(make_table_detail(), generation);

            let _ = session.begin_table_detail_run();

            assert!(session.table_detail().is_none());
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Loading
            ));
        }

        #[test]
        fn starting_detail_run_keeps_current_query_result_visible() {
            let mut session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let generation = session.select_table("public", "users", &mut query);
            let _ = session.set_table_detail(make_table_detail(), generation);
            query.set_current_result(make_query_result());

            let _ = session.begin_table_detail_run();

            assert!(query.current_result().is_some());
            assert!(matches!(
                session.table_detail_state(),
                TableDetailState::Loading
            ));
        }
    }

    // ── clear_table_selection ────────────────────────────────────────

    #[test]
    fn clear_table_selection_clears_all() {
        let mut session = BrowseSession::default();
        let mut query = QueryExecution::default();
        let _ = session.select_table("public", "users", &mut query);
        let _ = session.set_table_detail(make_table_detail(), session.selection_generation());
        query.set_current_result(make_query_result());

        session.clear_table_selection(&mut query);

        assert!(session.selected_table_key().is_none());
        assert!(session.table_detail().is_none());
        assert!(matches!(
            session.table_detail_state(),
            TableDetailState::NotSelected
        ));
        assert!(query.current_result().is_none());
        assert_eq!(query.pagination.current_page(), 0);
    }

    #[test]
    fn clear_table_selection_invalidates_pending_detail() {
        let mut session = BrowseSession::default();
        let mut query = QueryExecution::default();
        let pre_clear_gen = session.select_table("public", "users", &mut query);

        session.clear_table_selection(&mut query);

        // A TableDetailLoaded arriving with the pre-clear generation must be rejected
        let accepted = session.set_table_detail(make_table_detail(), pre_clear_gen);
        assert!(!accepted);
        assert!(session.table_detail().is_none());
        assert!(matches!(
            session.table_detail_state(),
            TableDetailState::NotSelected
        ));
    }

    #[test]
    fn clear_table_selection_terminates_active_query_run() {
        let mut session = BrowseSession::default();
        let mut query = QueryExecution::default();
        let run_id = query.begin_running(std::time::Instant::now());

        session.clear_table_selection(&mut query);

        assert!(!query.is_running());
        assert!(!query.is_current_run(run_id));
    }

    // ── Connection lifecycle ─────────────────────────────────────────

    mod connection_lifecycle {
        use super::*;

        #[test]
        fn begin_connecting_sets_pair() {
            let mut session = BrowseSession::default();

            let _ = session.begin_connecting("postgres://localhost/test");

            assert!(session.connection_state().is_connecting());
            assert_eq!(session.metadata_state(), &MetadataState::Loading);
            assert_eq!(session.dsn(), Some("postgres://localhost/test"));
        }

        #[test]
        fn mark_connecting_sets_pair_without_changing_dsn() {
            let mut session = BrowseSession::default();
            session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "postgres",
                DatabaseType::PostgreSQL,
                "postgres://localhost/test",
            );
            session.mark_effective_user_loaded(Some("old_user".to_string()));

            session.mark_connecting();

            assert!(session.connection_state().is_connecting());
            assert_eq!(session.metadata_state(), &MetadataState::Loading);
            assert_eq!(session.dsn(), Some("postgres://localhost/test"));
            assert!(session.effective_user().is_none());
        }

        #[test]
        fn activate_connection_with_dsn_disables_read_only() {
            let mut session = BrowseSession::default();
            let id = ConnectionId::new();
            session.enable_read_only();

            session.activate_connection_with_dsn(
                &id,
                "postgres",
                DatabaseType::PostgreSQL,
                "postgres://localhost/test",
            );

            assert!(!session.is_read_only());
            assert_eq!(session.dsn(), Some("postgres://localhost/test"));
            assert_eq!(session.active_connection_id(), Some(&id));
            assert_eq!(session.active_connection_name(), Some("postgres"));
            assert_eq!(
                session.active_database_type(),
                Some(DatabaseType::PostgreSQL)
            );
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
            assert!(!session.is_reloading());
        }

        #[test]
        fn mark_connection_failed_when_connected_keeps_connected() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let _ = session.begin_reload();
            session.mark_effective_user_loaded(Some("postgres".to_string()));

            session.mark_connection_failed("reload timeout".to_string());

            assert!(session.connection_state().is_connected());
            assert_eq!(
                session.metadata_state(),
                &MetadataState::Error("reload timeout".to_string())
            );
            assert!(!session.is_reloading());
            assert_eq!(session.effective_user(), Some("postgres"));
        }

        #[test]
        fn effective_user_completion_updates_state() {
            let mut session = BrowseSession::default();
            let run_id = session.begin_effective_user_fetch();

            assert!(session.is_current_effective_user_run(run_id));
            session.mark_effective_user_loaded(Some("postgres".to_string()));

            assert_eq!(session.effective_user(), Some("postgres"));
            assert!(!session.is_current_effective_user_run(run_id));
        }

        #[test]
        fn begin_metadata_refresh_keeps_connection_state() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));

            let _ = session.begin_metadata_refresh();

            assert!(session.connection_state().is_connected());
            assert_eq!(session.metadata_state(), &MetadataState::Loading);
        }

        #[test]
        fn mark_disconnected_resets_connection_pair() {
            let mut session = BrowseSession::default();
            let _ = session.begin_connecting("postgres://localhost/test");
            let _ = session.begin_reload();

            session.mark_disconnected();

            assert!(session.connection_state().is_not_connected());
            assert_eq!(session.metadata_state(), &MetadataState::NotLoaded);
            assert!(!session.is_reloading());
        }

        #[test]
        fn begin_reload_and_finish_reload() {
            let mut session = BrowseSession::default();

            let _ = session.begin_reload();
            assert!(session.is_reloading());

            session.finish_reload();
            assert!(!session.is_reloading());
        }
    }

    // ── to_cache / restore_from_cache round-trip ─────────────────────

    mod cache_round_trip {
        use super::*;

        #[test]
        fn round_trip_preserves_state() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("round_trip_db"));
            session.mark_effective_user_loaded(Some("postgres".to_string()));
            let mut query = QueryExecution::default();
            let _ = session.select_table("public", "users", &mut query);
            let _ = session.set_table_detail(make_table_detail(), session.selection_generation());

            let result = make_query_result();
            let mut history = ResultHistory::default();
            history.push(result.clone());
            query
                .pagination
                .reset_for_table_with_estimate("public", "users", Some(1200));
            query.pagination.set_page_result(2, false);

            let cache = session.to_cache(
                5,
                InspectorTab::Indexes,
                Some(result),
                history,
                query.pagination.clone(),
            );

            // Create a fresh session and restore
            let mut new_session = BrowseSession::default();
            let mut query = QueryExecution::default();
            let stale_run_id = query.begin_running(std::time::Instant::now());
            new_session.restore_from_cache(&cache, &mut query);

            assert_eq!(new_session.database_name(), Some("round_trip_db"));
            assert_eq!(new_session.effective_user(), Some("postgres"));
            assert!(new_session.table_detail().is_some());
            assert_eq!(new_session.selected_table_key(), Some("public.users"));
            assert!(new_session.connection_state().is_connected());
            assert_eq!(new_session.metadata_state(), &MetadataState::Loaded);
            assert!(query.current_result().is_some());
            assert_eq!(query.result_history().len(), 1);
            assert_eq!(query.pagination.schema(), "public");
            assert_eq!(query.pagination.table(), "users");
            assert_eq!(query.pagination.current_page(), 2);
            assert_eq!(query.pagination.total_rows_estimate(), Some(1200));
            assert!(!query.pagination.reached_end());
            assert!(!query.is_running());
            assert!(!query.is_current_run(stale_run_id));
        }

        #[test]
        fn restore_resets_generation_and_reloading() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let mut query = QueryExecution::default();
            let _ = session.select_table("public", "users", &mut query);
            let _ = session.begin_reload();
            assert!(session.selection_generation() > 0);

            let cache = session.to_cache(
                0,
                InspectorTab::Info,
                None,
                ResultHistory::default(),
                PaginationState::default(),
            );

            let mut new_session = BrowseSession::default();
            new_session.set_selection_generation(42);
            let _ = new_session.begin_reload();
            let mut query = QueryExecution::default();
            new_session.restore_from_cache(&cache, &mut query);

            assert_eq!(new_session.selection_generation(), 0);
            assert!(!new_session.is_reloading());
        }

        #[test]
        fn restore_without_table_detail_does_not_claim_loading() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let mut query = QueryExecution::default();
            let _ = session.select_table("public", "users", &mut query);

            let cache = session.to_cache(
                0,
                InspectorTab::Info,
                None,
                ResultHistory::default(),
                PaginationState::default(),
            );

            let mut restored = BrowseSession::default();
            let mut query = QueryExecution::default();
            restored.restore_from_cache(&cache, &mut query);

            assert_eq!(restored.selected_table_key(), Some("public.users"));
            assert!(matches!(
                restored.table_detail_state(),
                TableDetailState::NotSelected
            ));
            assert!(!restored.is_current_table_detail_run(1));
        }

        #[test]
        fn restore_then_begin_reload_preserves_selection() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            let mut query = QueryExecution::default();
            let generation = session.select_table("public", "users", &mut query);
            let _ = session.set_table_detail(make_table_detail(), generation);

            let cache = session.to_cache(
                3,
                InspectorTab::Columns,
                Some(make_query_result()),
                ResultHistory::default(),
                PaginationState::default(),
            );

            let mut restored = BrowseSession::default();
            let mut query = QueryExecution::default();
            restored.restore_from_cache(&cache, &mut query);
            let _ = restored.begin_reload();

            assert_eq!(restored.selected_table_key(), Some("public.users"));
            assert!(restored.table_detail().is_some());
            assert!(restored.is_reloading());
            assert!(restored.connection_state().is_connected());
        }
    }

    // ── reset ────────────────────────────────────────────────────────

    mod reset_tests {
        use super::*;

        #[test]
        fn clears_session_and_query_state() {
            let mut session = BrowseSession::default();
            session.mark_connected(make_metadata("db"));
            session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "mydb",
                DatabaseType::PostgreSQL,
                "postgres://host/db",
            );
            session.enable_read_only();
            let _ = session.begin_reload();
            let mut query = QueryExecution::default();
            let stale_run_id = query.begin_running(std::time::Instant::now());
            query.set_current_result(make_query_result());
            query.pagination.reset_for_table("public", "users");
            query.pagination.set_total_rows_estimate(Some(1000));
            query.pagination.set_page_result(3, true);
            session.reset(&mut query);

            assert!(session.connection_state().is_not_connected());
            assert_eq!(session.metadata_state(), &MetadataState::NotLoaded);
            assert!(session.metadata().is_none());
            assert!(session.database_name().is_none());
            assert!(session.selected_table_key().is_none());
            assert!(session.table_detail().is_none());
            assert_eq!(session.selection_generation(), 0);
            assert!(session.dsn().is_none());
            assert!(session.active_connection_id().is_none());
            assert!(session.active_connection_name().is_none());
            assert!(session.active_database_type().is_none());
            assert_eq!(
                session.active_engine_feature_profile(),
                &EngineFeatureProfile::disconnected()
            );
            assert!(!session.is_read_only());
            assert!(!session.is_reloading());
            assert!(session.effective_user().is_none());
            assert_eq!(query.pagination.current_page(), 0);
            assert!(query.current_result().is_none());
            assert!(!query.is_running());
            assert!(!query.is_current_run(stale_run_id));
        }

        #[test]
        fn preserves_metadata_run_counter_after_reset() {
            let mut session = BrowseSession::default();
            let first_run_id = session.begin_connecting("postgres://localhost/test");
            assert_eq!(first_run_id, 1);

            let mut query = QueryExecution::default();
            session.reset(&mut query);

            let second_run_id = session.begin_connecting("postgres://localhost/test");
            assert_eq!(second_run_id, 2);
            assert!(!session.is_current_metadata_run(first_run_id));
            assert!(session.is_current_metadata_run(second_run_id));
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
        fn name_after_mark_connected() {
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

            let cache = session.to_cache(
                0,
                InspectorTab::Info,
                None,
                ResultHistory::default(),
                PaginationState::default(),
            );

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
            query.pagination.reset_for_table("public", "old_table");
            query.pagination.set_page_result(4, true);
            session.restore_from_cache(&cache, &mut query);

            assert!(session.database_name().is_none());
            assert_eq!(query.pagination.current_page(), 0);
            assert!(query.pagination.schema().is_empty());
            assert!(query.pagination.table().is_empty());
            assert!(query.pagination.total_rows_estimate().is_none());
            assert!(!query.pagination.reached_end());
        }
    }

    mod query_history_scope_tests {
        use super::*;

        #[test]
        fn mysql_scope_includes_selected_database() {
            let mut session = BrowseSession::default();
            session.activate_connection_with_target(
                &ConnectionId::from_string("mysql"),
                "mysql",
                DatabaseType::MySQL,
                "mysql://localhost/app",
                Some("app"),
            );

            let scope = session.query_history_scope().unwrap();

            assert_eq!(scope.database.as_deref(), Some("app"));
        }

        #[test]
        fn postgres_and_sqlite_scopes_omit_database() {
            for database_type in [DatabaseType::PostgreSQL, DatabaseType::SQLite] {
                let mut session = BrowseSession::default();
                session.activate_connection_with_target(
                    &ConnectionId::from_string("connection"),
                    "connection",
                    database_type,
                    "dsn://connection",
                    Some("database"),
                );

                assert_eq!(session.query_history_scope().unwrap().database, None);
            }
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
        fn is_ephemeral_connection_detects_cli_connection() {
            use crate::cmd::cli_sqlite::connection_id_for_path;

            let mut session = BrowseSession::default();
            session.activate_cli_ephemeral_connection(
                &connection_id_for_path("/tmp/app.db"),
                "app.db",
                "sqlite:///tmp/app.db",
            );

            assert!(session.is_ephemeral_connection());
            assert!(!session.can_reenter_connection_setup());
        }

        #[test]
        fn can_reenter_connection_setup_for_saved_profile() {
            let mut session = BrowseSession::default();
            session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "Local",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );

            assert!(!session.is_ephemeral_connection());
            assert!(session.can_reenter_connection_setup());
        }

        #[test]
        fn pending_probe_debug_masks_password() {
            let mut session = BrowseSession::default();
            let id = ConnectionId::new();
            let _ = session.begin_mysql_connection_probe(
                &id,
                "mysql",
                DatabaseType::MySQL,
                "mysql://user:secret@localhost:3306/app",
                Some("app"),
            );

            let debug = format!("{session:?}");
            assert!(!debug.contains("secret"));
            assert!(debug.contains("mysql://user:****@localhost:3306/app"));
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

        #[test]
        fn active_database_type_or_default_returns_postgresql_when_none() {
            let session = BrowseSession::default();

            assert_eq!(
                session.active_database_type_or_default(),
                DatabaseType::PostgreSQL
            );
        }
    }
}
