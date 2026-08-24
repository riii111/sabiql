use std::fmt;
use std::sync::Arc;

use crate::domain::connection::{
    ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType, ServiceEntry,
};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::Prefix;
use crate::model::sql_editor::completion::CompletionCandidate;
use crate::policy::{FeatureRequirement, mask_password};
use crate::ports::outbound::clipboard::ClipboardError;
use crate::ports::outbound::connection_store::ConnectionStoreError;
use crate::ports::outbound::folder_opener::FolderOpenError;
use crate::ports::outbound::query_history::QueryHistoryError;
use crate::ports::outbound::settings_store::SettingsStoreError;
use crate::ports::outbound::{AppSettings, DbOperationError};
use std::collections::HashMap;

use crate::domain::SqliteDiagnosticsSnapshot;
use crate::domain::{
    DatabaseDiagnostic, DatabaseMetadata, DiagnosticField, QueryResult, Table,
    TableSignatureSnapshot,
};

#[derive(Clone, thiserror::Error)]
pub enum ConnectionSaveError {
    #[error("{0}")]
    Validation(#[from] ConnectionProfileError),
    #[error("{0}")]
    Store(#[from] ConnectionStoreError),
    #[error("{0}")]
    Metadata(#[from] DbOperationError),
    #[error("{error}")]
    Probe {
        error: DbOperationError,
        dsn: String,
    },
}

impl fmt::Debug for ConnectionSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => formatter.debug_tuple("Validation").field(error).finish(),
            Self::Store(error) => formatter.debug_tuple("Store").field(error).finish(),
            Self::Metadata(error) => formatter.debug_tuple("Metadata").field(error).finish(),
            Self::Probe { error, dsn } => formatter
                .debug_struct("Probe")
                .field("error", error)
                .field("dsn", &mask_password(dsn))
                .finish(),
        }
    }
}

pub use crate::model::shared::cursor::CursorMove;

// ---------------------------------------------------------------------------
// Parametric Action types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Result,
    Inspector,
    Help,
    ConnectionError,
    ConfirmDialog,
    ExplainPlan,
    ExplainCompare,
    ExplainConfirm,
    Explorer,
    JsonDetail,
    CellDetail,
    SqliteDiagnostics,
    RowDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDirection {
    pub fn clamp_vertical_offset(self, current: usize, max: usize, delta: usize) -> usize {
        match self {
            Self::Down => (current + delta).min(max),
            Self::Up => current.saturating_sub(delta),
            Self::Left | Self::Right => current,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAmount {
    Line,
    ToStart,
    ToEnd,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    HalfPage,
    FullPage,
}

impl ScrollAmount {
    pub fn page_delta(self, visible: usize) -> Option<usize> {
        if visible == 0 {
            return None;
        }

        Some(match self {
            Self::HalfPage => (visible / 2).max(1),
            Self::FullPage => visible,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollToCursorTarget {
    Explorer,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    Center,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    SqlModal,
    SqlModalHighRisk,
    SqlModalAnalyzeHighRisk,
    ResultCellEdit,
    ConnectionSetup,
    CommandLine,
    Filter,
    ErFilter,
    SettingsErBrowser,
    QueryHistoryFilter,
    JsonEdit,
    JsonSearch,
    CellDetailSearch,
    HelpFilter,
}

pub use crate::model::shared::text_input::TextKillDirection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMotion {
    Next,
    Previous,
    First,
    Last,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    HalfPageDown,
    HalfPageUp,
    FullPageDown,
    FullPageUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTarget {
    ConnectionList,
    QueryHistory,
    TablePicker,
    ErTablePicker,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMotion {
    Next,
    Previous,
}

/// Payload-free modal lifecycle kinds.
///
/// Modal lifecycle actions stay generic only while opening does not need a
/// payload. Payload-bearing modals keep explicit actions until there is a
/// repeated payload pattern worth abstracting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    TablePicker,
    CommandPalette,
    Settings,
    Help,
    SqlModal,
    ErTablePicker,
    QueryHistoryPicker,
    JsonDetail,
    CellDetail,
    RowDetail,
    ConnectionSetup,
    ConnectionSelector,
    SqliteDiagnostics,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshResult {
    pub dsn: String,
    pub run_id: u64,
    pub new_metadata: Arc<DatabaseMetadata>,
    pub stale_tables: Vec<String>,
    pub added_tables: Vec<String>,
    pub removed_tables: Vec<String>,
    pub missing_in_cache: Vec<String>,
    pub new_signatures: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshFetched {
    pub dsn: String,
    pub run_id: u64,
    pub new_metadata: Arc<DatabaseMetadata>,
    pub signature_snapshot: Arc<TableSignatureSnapshot>,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshError {
    pub dsn: String,
    pub run_id: u64,
    pub error: DbOperationError,
    pub new_metadata: Option<Arc<DatabaseMetadata>>,
}

#[derive(Debug, Clone)]
pub struct ErDiagramInfo {
    pub run_id: u64,
    pub path: String,
    pub table_count: usize,
    pub total_tables: usize,
}

#[derive(Debug, Clone)]
pub struct ConnectionsLoadedPayload {
    pub profiles: Vec<ConnectionProfile>,
    pub services: Vec<ServiceEntry>,
    pub service_file_path: Option<std::path::PathBuf>,
    pub profile_load_warning: Option<String>,
    pub service_load_warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TableTarget {
    pub schema: String,
    pub table: String,
    pub generation: u64,
}

#[derive(Clone)]
pub struct ConnectionTarget {
    pub id: ConnectionId,
    pub dsn: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub database: Option<String>,
}

impl ConnectionTarget {
    pub fn from_profile(profile: &ConnectionProfile, dsn: String) -> Self {
        Self {
            id: profile.id.clone(),
            dsn,
            name: profile.display_name().to_string(),
            database_type: profile.database_type(),
            database: profile
                .mysql_config()
                .and_then(|config| config.database.clone()),
        }
    }
}

impl fmt::Debug for ConnectionTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionTarget")
            .field("id", &self.id)
            .field("dsn", &mask_password(&self.dsn))
            .field("name", &self.name)
            .field("database_type", &self.database_type)
            .field("database", &self.database)
            .finish()
    }
}

// Full Action equality is intentionally unavailable: some payloads carry
// snapshots or errors that are not value-comparable.
//
// Classification rule:
// Order shared controls first, then product objects by dependency:
// setup -> DB structure -> SQL -> query results -> result derivatives.
// Product groups follow the object in the action sentence, not UI/reducer names
// (e.g., MetadataLoaded -> Database structure, QueryCompleted -> Query results).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryCompletionContext {
    Adhoc,
    Preview { generation: u64, target_page: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryFailureContext {
    Adhoc,
    Preview { generation: u64 },
}

#[derive(Debug, Clone)]
pub enum Action {
    // App shell
    None,
    Quit,
    Render,
    Resize(u16, u16),
    SetFocusedPane(FocusedPane),

    // Input primitives
    Scroll {
        target: ScrollTarget,
        direction: ScrollDirection,
        amount: ScrollAmount,
    },
    ScrollToCursor {
        target: ScrollToCursorTarget,
        position: CursorPosition,
    },
    TextInput {
        target: InputTarget,
        ch: char,
    },
    TextBackspace {
        target: InputTarget,
    },
    TextDelete {
        target: InputTarget,
    },
    TextKill {
        target: InputTarget,
        direction: TextKillDirection,
    },
    TextYank {
        target: InputTarget,
    },
    TextMoveCursor {
        target: InputTarget,
        direction: CursorMove,
    },
    EnterHelpFilter,
    ExitHelpFilter,
    Select(SelectMotion),
    ListSelect {
        target: ListTarget,
        motion: ListMotion,
    },
    Paste(String),
    BeginKeySequence(Prefix),
    CancelKeySequence,

    // Modal shell
    OpenModal(ModalKind),
    CloseModal(ModalKind),
    ToggleModal(ModalKind),
    Escape,
    ConfirmSelection,
    ConfirmDialogConfirm,
    ConfirmDialogCancel,

    // Command line
    EnterCommandLine,
    ExitCommandLine,
    CommandLineSubmit,

    // Connections
    TryConnect,
    SwitchConnection(ConnectionTarget),
    ConnectionsLoaded(ConnectionsLoadedPayload),
    ConfirmConnectionSelection,
    StartEditConnection(ConnectionId),
    ConnectionSetupNextField,
    ConnectionSetupPrevField,
    ConnectionSetupToggleDropdown,
    ConnectionSetupDropdownNext,
    ConnectionSetupDropdownPrev,
    ConnectionSetupDropdownConfirm,
    ConnectionSetupDropdownCancel,
    ConnectionSetupSave,
    ConnectionSetupCancel,
    ConnectionSaveCompleted {
        target: ConnectionTarget,
        run_id: u64,
        mysql_lower_case_table_names: Option<u8>,
    },
    ConnectionSaveFailed {
        error: ConnectionSaveError,
        database_type: DatabaseType,
        run_id: u64,
    },
    MySqlConnectionProbeCompleted {
        target: ConnectionTarget,
        run_id: u64,
        lower_case_table_names: u8,
    },
    MySqlConnectionProbeFailed {
        target: ConnectionTarget,
        run_id: u64,
        error: DbOperationError,
    },
    ConnectionEditLoaded(Box<ConnectionProfile>),
    ConnectionEditLoadFailed(ConnectionStoreError),
    CloseConnectionError,
    ToggleConnectionErrorDetails,
    CopyConnectionError,
    ConnectionErrorCopied,
    ReenterConnectionSetup,
    RetryConnection,
    RequestDeleteSelectedConnection,
    DeleteConnection(ConnectionId),
    ConnectionDeleted(ConnectionId),
    ConnectionDeleteFailed(ConnectionStoreError),
    RequestEditSelectedConnection,

    // SQLite diagnostics
    RunSqliteDiagnosticsQuickCheck,
    SqliteDiagnosticsCoreLoaded {
        dsn: String,
        run_id: u64,
        snapshot: Box<SqliteDiagnosticsSnapshot>,
    },
    SqliteDiagnosticsQuickCheckLoaded {
        dsn: String,
        run_id: u64,
        quick_check: DiagnosticField,
    },

    // Settings
    SettingsSelectNext,
    SettingsSelectPrevious,
    SettingsNextSection,
    SettingsPreviousSection,
    SettingsStartCustomBrowserEdit,
    SettingsStopCustomBrowserEdit,
    SettingsApply,
    SettingsCancel,
    SettingsSaved(AppSettings),
    SettingsSaveFailed(SettingsStoreError),

    // Database structure
    LoadMetadata,
    ReloadMetadata,
    MetadataLoaded {
        dsn: String,
        run_id: u64,
        metadata: Arc<DatabaseMetadata>,
    },
    MetadataFailed {
        dsn: String,
        run_id: u64,
        error: DbOperationError,
    },
    EffectiveUserLoaded {
        dsn: String,
        run_id: u64,
        effective_user: Option<String>,
    },
    TableDetailLoaded {
        dsn: String,
        run_id: u64,
        detail: Box<Table>,
        generation: u64,
    },
    TableDetailFailed {
        dsn: String,
        run_id: u64,
        error: DbOperationError,
        generation: u64,
    },
    PrefetchTableDetail {
        run_id: u64,
        schema: String,
        table: String,
    },
    TableDetailCached {
        dsn: String,
        run_id: u64,
        schema: String,
        table: String,
        detail: Box<Table>,
    },
    TableDetailCacheFailed {
        dsn: String,
        run_id: u64,
        schema: String,
        table: String,
        error: DbOperationError,
    },
    TableDetailAlreadyCached {
        dsn: String,
        run_id: u64,
        schema: String,
        table: String,
    },
    StartPrefetchAll,
    StartPrefetchScoped {
        tables: Vec<String>,
    },
    StartCompletionPrefetch {
        tables: Vec<String>,
    },
    ExpandPrefetchWithFkNeighbors {
        run_id: u64,
    },
    FkNeighborsDiscovered {
        run_id: u64,
        tables: Vec<String>,
    },
    ProcessPrefetchQueue {
        run_id: u64,
    },
    InspectorNextTab,
    InspectorPrevTab,

    // SQL editing
    SqlModalAppendInsert,
    SqlModalEnterInsert,
    SqlModalEnterNormal,
    SqlModalYank,
    SqlModalYankSuccess,
    SqlModalNewLine,
    SqlModalTab,
    SqlModalSubmit,
    SqlModalClear,
    SqlModalCancelConfirm,
    SqlModalConfirmExecute,
    SqlModalNextTab,
    SqlModalPrevTab,
    CompletionRequest,
    CompletionUpdated {
        candidates: Vec<CompletionCandidate>,
        trigger_position: usize,
        visible: bool,
        dsn: Option<String>,
        connection_generation: u64,
        database_generation: u64,
        metadata_generation: u64,
    },
    CompletionAccept,
    CompletionDismiss,
    CompletionNext,
    CompletionPrev,

    // Explain plans
    ExplainRequest,
    ExplainAnalyzeRequest,
    ExplainAnalyzeConfirm,
    ExplainAnalyzeCancel,
    ExplainCompleted {
        dsn: String,
        database_type: DatabaseType,
        database_generation: u64,
        run_id: u64,
        query: String,
        plan_text: String,
        is_analyze: bool,
        execution_time_ms: u64,
    },
    ExplainFailed {
        dsn: String,
        database_generation: u64,
        run_id: u64,
        error: DbOperationError,
        is_analyze: bool,
    },
    CompareEditQuery,

    // Query results
    ExecutePreview(TableTarget),
    ExecuteAdhoc(String),
    ExecuteWrite(String),
    QueryCompleted {
        dsn: String,
        run_id: u64,
        result: Arc<QueryResult>,
        context: QueryCompletionContext,
    },
    QueryFailed {
        dsn: String,
        run_id: u64,
        error: DbOperationError,
        context: QueryFailureContext,
    },
    RevealPendingPreview {
        generation: u64,
    },
    ExecuteWriteSucceeded {
        dsn: String,
        run_id: u64,
        affected_rows: usize,
        diagnostics: Vec<DatabaseDiagnostic>,
    },
    ExecuteWriteFailed {
        dsn: String,
        run_id: u64,
        error: DbOperationError,
    },
    ResultNextPage,
    ResultPrevPage,
    ResultActivateCell,
    ResultExitToScroll,
    ResultCellLeft,
    ResultCellRight,
    ResultCellYank,
    ResultCellYankSuccess {
        row: usize,
        col: usize,
    },
    ResultRowYankOperatorPending,
    ResultRowYank,
    ResultRowYankSuccess {
        row: usize,
    },
    DdlYank,
    DdlYankSuccess,
    ResultDeleteOperatorPending,
    StageRowForDelete,
    UnstageLastStagedRow,
    ClearStagedDeletes,
    RequestDeleteActiveRow,
    ResultEnterCellEdit,
    ResultOpenCellDetail,
    ResultCancelCellEdit,
    ResultDiscardCellEdit,
    SubmitCellEditWrite,
    CopyFailed(ClipboardError),
    OpenFolderFailed(FolderOpenError),
    ToggleFocus,
    ToggleReadOnly,

    // Query history
    QueryHistoryLoaded(QueryHistoryScope, Vec<QueryHistoryEntry>),
    QueryHistoryLoadFailed(QueryHistoryScope, QueryHistoryError),
    QueryHistoryConfirmSelection,

    // CSV export
    RequestCsvExport,
    CsvExportRowsCounted {
        dsn: String,
        run_id: u64,
        row_count: Option<usize>,
        export_query: String,
        file_name: String,
    },
    ExecuteCsvExport {
        dsn: String,
        run_id: u64,
        export_query: String,
        file_name: String,
        row_count: Option<usize>,
    },
    CsvExportSucceeded {
        dsn: String,
        run_id: u64,
        path: String,
        row_count: Option<usize>,
    },
    CsvExportFailed {
        dsn: String,
        run_id: u64,
        error: DbOperationError,
    },

    // JSON detail
    JsonYankAll,
    JsonYankSuccess,
    JsonEnterEdit,
    JsonAppendInsert,
    JsonExitEdit,
    JsonEnterSearch,
    JsonExitSearch,
    JsonSearchNext,
    JsonSearchPrev,
    JsonSearchSubmit,

    // Cell detail
    CellDetailYankAll,
    CellDetailYankSuccess,
    CellDetailEnterSearch,
    CellDetailExitSearch,
    CellDetailSearchNext,
    CellDetailSearchPrev,
    CellDetailSearchSubmit,

    // Row Detail
    RowDetailYank,
    RowDetailYankJson,
    RowDetailYankSuccess,

    // ER diagrams
    ErToggleSelection,
    ErSelectAll,
    ErConfirmSelection,
    ErOpenDiagram,
    ErGenerateFromCache,
    SmartErRefreshFetched(SmartErRefreshFetched),
    SmartErRefreshCompleted(SmartErRefreshResult),
    SmartErRefreshFailed(SmartErRefreshError),
    ErDiagramOpened(ErDiagramInfo),
    ErDiagramFailed {
        run_id: u64,
        error: String,
    },
    ErLogWriteFailed(String),
}

impl Action {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll { .. })
    }

    pub fn feature_requirement(&self) -> FeatureRequirement {
        use FeatureRequirement::{
            ErDiagram, Explain, ExplainAnalyze, JsonDocumentDetail, JsonDocumentEdit, None,
            PlanComparison, SqliteDiagnostics,
        };

        match self {
            Self::OpenModal(ModalKind::ErTablePicker)
            | Self::ToggleModal(ModalKind::ErTablePicker)
            | Self::ErToggleSelection
            | Self::ErSelectAll
            | Self::ErConfirmSelection
            | Self::ErOpenDiagram
            | Self::ErGenerateFromCache
            | Self::SmartErRefreshFetched(_)
            | Self::SmartErRefreshCompleted(_)
            | Self::SmartErRefreshFailed(_)
            | Self::ErDiagramOpened(_)
            | Self::ErDiagramFailed { .. }
            | Self::ErLogWriteFailed(_)
            | Self::TextInput {
                target: InputTarget::ErFilter,
                ..
            }
            | Self::TextBackspace {
                target: InputTarget::ErFilter,
            }
            | Self::TextDelete {
                target: InputTarget::ErFilter,
            }
            | Self::TextKill {
                target: InputTarget::ErFilter,
                ..
            }
            | Self::TextYank {
                target: InputTarget::ErFilter,
            }
            | Self::TextMoveCursor {
                target: InputTarget::ErFilter,
                ..
            }
            | Self::ListSelect {
                target: ListTarget::ErTablePicker,
                ..
            } => ErDiagram,
            Self::OpenModal(ModalKind::SqliteDiagnostics)
            | Self::ToggleModal(ModalKind::SqliteDiagnostics)
            | Self::RunSqliteDiagnosticsQuickCheck
            | Self::SqliteDiagnosticsCoreLoaded { .. }
            | Self::SqliteDiagnosticsQuickCheckLoaded { .. }
            | Self::Scroll {
                target: ScrollTarget::SqliteDiagnostics,
                ..
            } => SqliteDiagnostics,
            Self::OpenModal(ModalKind::JsonDetail)
            | Self::ToggleModal(ModalKind::JsonDetail)
            | Self::JsonYankAll
            | Self::JsonYankSuccess
            | Self::JsonEnterSearch
            | Self::JsonExitSearch
            | Self::JsonSearchNext
            | Self::JsonSearchPrev
            | Self::JsonSearchSubmit
            | Self::TextInput {
                target: InputTarget::JsonSearch,
                ..
            }
            | Self::TextBackspace {
                target: InputTarget::JsonSearch,
            }
            | Self::TextDelete {
                target: InputTarget::JsonSearch,
            }
            | Self::TextKill {
                target: InputTarget::JsonSearch,
                ..
            }
            | Self::TextYank {
                target: InputTarget::JsonSearch,
            }
            | Self::TextMoveCursor {
                target: InputTarget::JsonEdit | InputTarget::JsonSearch,
                ..
            } => JsonDocumentDetail,
            Self::JsonEnterEdit
            | Self::JsonAppendInsert
            | Self::JsonExitEdit
            | Self::TextInput {
                target: InputTarget::JsonEdit,
                ..
            }
            | Self::TextBackspace {
                target: InputTarget::JsonEdit,
            }
            | Self::TextDelete {
                target: InputTarget::JsonEdit,
            }
            | Self::TextKill {
                target: InputTarget::JsonEdit,
                ..
            }
            | Self::TextYank {
                target: InputTarget::JsonEdit,
            } => JsonDocumentEdit,
            Self::ExplainRequest
            | Self::Scroll {
                target: ScrollTarget::ExplainPlan,
                ..
            }
            | Self::ExplainCompleted {
                is_analyze: false, ..
            }
            | Self::ExplainFailed {
                is_analyze: false, ..
            } => Explain,
            Self::ExplainAnalyzeRequest
            | Self::ExplainAnalyzeConfirm
            | Self::ExplainAnalyzeCancel
            | Self::TextInput {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                ..
            }
            | Self::TextBackspace {
                target: InputTarget::SqlModalAnalyzeHighRisk,
            }
            | Self::TextDelete {
                target: InputTarget::SqlModalAnalyzeHighRisk,
            }
            | Self::TextKill {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                ..
            }
            | Self::TextYank {
                target: InputTarget::SqlModalAnalyzeHighRisk,
            }
            | Self::TextMoveCursor {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                ..
            }
            | Self::Scroll {
                target: ScrollTarget::ExplainConfirm,
                ..
            }
            | Self::ExplainCompleted {
                is_analyze: true, ..
            }
            | Self::ExplainFailed {
                is_analyze: true, ..
            } => ExplainAnalyze,
            Self::CompareEditQuery
            | Self::Scroll {
                target: ScrollTarget::ExplainCompare,
                ..
            } => PlanComparison,
            _ => None,
        }
    }

    pub fn feature_requirement_for_state(&self, state: &AppState) -> FeatureRequirement {
        match self {
            Self::JsonExitEdit if state.input_mode() == InputMode::JsonEdit => {
                FeatureRequirement::None
            }
            Self::JsonExitSearch
                if state.input_mode() == InputMode::JsonDetail
                    && state.json_detail.mode() == JsonDetailMode::Searching =>
            {
                FeatureRequirement::None
            }
            Self::Paste(_) => match state.input_mode() {
                InputMode::ErTablePicker => FeatureRequirement::ErDiagram,
                InputMode::JsonDetail => FeatureRequirement::JsonDocumentDetail,
                InputMode::JsonEdit => FeatureRequirement::JsonDocumentEdit,
                _ => FeatureRequirement::None,
            },
            Self::BeginKeySequence(Prefix::G)
                if matches!(
                    state.input_mode(),
                    InputMode::JsonDetail | InputMode::JsonEdit
                ) =>
            {
                FeatureRequirement::JsonDocumentDetail
            }
            _ => self.feature_requirement(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::MySqlSslMode;
    use rstest::rstest;

    #[test]
    fn connection_target_from_profile_preserves_profile_mapping() {
        let profile = ConnectionProfile::new_mysql(
            "MySQL",
            "localhost",
            3306,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Required,
        )
        .unwrap();
        let target = ConnectionTarget::from_profile(&profile, "mysql://dsn".to_string());

        assert_eq!(target.id, profile.id);
        assert_eq!(target.dsn, "mysql://dsn");
        assert_eq!(target.name, profile.display_name());
        assert_eq!(target.database_type, profile.database_type());
        assert_eq!(target.database.as_deref(), Some("app"));
    }

    #[test]
    fn connection_target_debug_masks_mysql_password() {
        let target = ConnectionTarget {
            id: ConnectionId::from_string("mysql"),
            dsn: "mysql://user:secret@localhost:3306/app".to_string(),
            name: "MySQL".to_string(),
            database_type: DatabaseType::MySQL,
            database: Some("app".to_string()),
        };

        let debug = format!("{target:?}");

        assert!(!debug.contains("secret"));
        assert!(debug.contains("mysql://user:****@localhost"));
    }

    #[test]
    fn connection_save_probe_debug_masks_mysql_password() {
        let error = ConnectionSaveError::Probe {
            error: DbOperationError::ConnectionFailed("probe failed".to_string()),
            dsn: "mysql://user:secret@localhost:3306/app".to_string(),
        };

        let debug = format!("{error:?}");

        assert!(!debug.contains("secret"));
        assert!(debug.contains("mysql://user:****@localhost"));
    }

    #[test]
    fn scroll_action_returns_true() {
        let action = Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        };
        assert!(action.is_scroll());
    }

    #[rstest]
    #[case(Action::None)]
    #[case(Action::Quit)]
    #[case(Action::Render)]
    #[case(Action::ScrollToCursor {
        target: ScrollToCursorTarget::Result,
        position: CursorPosition::Center,
    })]
    fn non_scroll_action_returns_false(#[case] action: Action) {
        assert!(!action.is_scroll());
    }

    #[test]
    fn completion_actions_keep_feature_requirements() {
        assert_eq!(
            Action::ExplainCompleted {
                dsn: "dsn".to_string(),
                database_type: DatabaseType::PostgreSQL,
                database_generation: 0,
                run_id: 1,
                query: "SELECT 1".to_string(),
                plan_text: "plan".to_string(),
                is_analyze: true,
                execution_time_ms: 1,
            }
            .feature_requirement(),
            FeatureRequirement::ExplainAnalyze
        );
        assert_eq!(
            Action::ExplainFailed {
                dsn: "dsn".to_string(),
                database_generation: 0,
                run_id: 1,
                error: DbOperationError::QueryFailed("error".to_string()),
                is_analyze: false,
            }
            .feature_requirement(),
            FeatureRequirement::Explain
        );
        assert_eq!(
            Action::SqliteDiagnosticsCoreLoaded {
                dsn: "sqlite://test.db".to_string(),
                run_id: 1,
                snapshot: Box::new(SqliteDiagnosticsSnapshot::default()),
            }
            .feature_requirement(),
            FeatureRequirement::SqliteDiagnostics
        );
        assert_eq!(
            Action::ExplainAnalyzeCancel.feature_requirement(),
            FeatureRequirement::ExplainAnalyze
        );
        assert_eq!(
            Action::Scroll {
                target: ScrollTarget::ExplainCompare,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
            .feature_requirement(),
            FeatureRequirement::PlanComparison
        );
        assert_eq!(
            Action::TextInput {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                ch: 'x',
            }
            .feature_requirement(),
            FeatureRequirement::ExplainAnalyze
        );
    }

    #[test]
    fn json_cleanup_actions_are_allowed_on_preserved_surfaces() {
        let mut edit_state = AppState::new("test".to_string());
        edit_state.modal.set_mode(InputMode::JsonEdit);
        assert_eq!(
            Action::JsonExitEdit.feature_requirement_for_state(&edit_state),
            FeatureRequirement::None
        );

        let mut search_state = AppState::new("test".to_string());
        search_state.modal.set_mode(InputMode::JsonDetail);
        search_state.json_detail.enter_search();
        assert_eq!(
            Action::JsonExitSearch.feature_requirement_for_state(&search_state),
            FeatureRequirement::None
        );

        let normal_state = AppState::new("test".to_string());
        assert_eq!(
            Action::JsonExitEdit.feature_requirement_for_state(&normal_state),
            FeatureRequirement::JsonDocumentEdit
        );
    }

    mod shared_scroll_helpers {
        use super::*;

        #[rstest]
        #[case(ScrollDirection::Down, 3, 10, 4, 7)]
        #[case(ScrollDirection::Down, 3, 5, 10, 5)]
        #[case(ScrollDirection::Down, 3, 0, 10, 0)]
        #[case(ScrollDirection::Up, 8, 10, 3, 5)]
        #[case(ScrollDirection::Up, 3, 10, 5, 0)]
        #[case(ScrollDirection::Up, 3, 0, 5, 0)]
        #[case(ScrollDirection::Left, 4, 10, 6, 4)]
        #[case(ScrollDirection::Right, 4, 10, 6, 4)]
        fn clamp_vertical_offset_handles_boundaries(
            #[case] direction: ScrollDirection,
            #[case] current: usize,
            #[case] max: usize,
            #[case] delta: usize,
            #[case] expected: usize,
        ) {
            assert_eq!(
                direction.clamp_vertical_offset(current, max, delta),
                expected
            );
        }

        #[rstest]
        #[case(ScrollAmount::HalfPage, 0, None)]
        #[case(ScrollAmount::HalfPage, 1, Some(1))]
        #[case(ScrollAmount::HalfPage, 17, Some(8))]
        #[case(ScrollAmount::FullPage, 0, None)]
        #[case(ScrollAmount::FullPage, 1, Some(1))]
        #[case(ScrollAmount::FullPage, 17, Some(17))]
        #[case(ScrollAmount::Line, 17, None)]
        fn page_delta_respects_visible_rows(
            #[case] amount: ScrollAmount,
            #[case] visible: usize,
            #[case] expected: Option<usize>,
        ) {
            assert_eq!(amount.page_delta(visible), expected);
        }
    }
}
