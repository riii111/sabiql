use std::fmt;
use std::sync::Arc;

use crate::domain::connection::{
    ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType, ServiceEntry,
};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::Prefix;
use crate::policy::{FeatureRequirement, mask_password};
use crate::ports::outbound::clipboard::ClipboardError;
use crate::ports::outbound::{AppSettings, DbOperationError};

use crate::domain::SqliteDiagnosticsSnapshot;
use crate::domain::{
    DatabaseDiagnostic, DatabaseMetadata, DiagnosticField, QueryResult, Table,
    TableSignatureSnapshot,
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionSaveError {
    #[error("{0}")]
    Validation(#[from] ConnectionProfileError),
    #[error("{0}")]
    Store(String),
    #[error("{0}")]
    Metadata(#[from] DbOperationError),
    #[error("{error}")]
    Probe { error: DbOperationError },
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
    CellDetail,
    SqliteDiagnostics,
    RowDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTarget {
    JsonDetail,
    CellDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStep {
    Enter,
    Exit,
    Next,
    Prev,
    Submit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyTarget {
    Panel(FlashId),
    ResultCell { row: usize, col: usize },
    ResultRow { row: usize },
    ConnectionError,
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

pub use crate::model::shared::text_input::{TextEdit, TextKillDirection};

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
    ToggleHelp,
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
        metadata: Option<Arc<DatabaseMetadata>>,
    },
    ConnectionSaveFailed {
        error: ConnectionSaveError,
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
    ConnectionEditLoadFailed(String),
    CloseConnectionError,
    ToggleConnectionErrorDetails,
    CopyConnectionError,
    ReenterConnectionSetup,
    RetryConnection,
    RequestDeleteSelectedConnection,
    DeleteConnection(ConnectionId),
    ConnectionDeleted(ConnectionId),
    ConnectionDeleteFailed(String),
    RequestEditSelectedConnection,

    // SQLite diagnostics
    RunSqliteDiagnosticsQuickCheck,
    SqliteDiagnosticsCoreLoaded {
        run_id: u64,
        snapshot: Box<SqliteDiagnosticsSnapshot>,
    },
    SqliteDiagnosticsQuickCheckLoaded {
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
    SettingsSaveFailed(String),

    // Database structure
    ReloadMetadata,
    MetadataLoaded {
        run_id: u64,
        metadata: Arc<DatabaseMetadata>,
    },
    MetadataFailed {
        run_id: u64,
        error: DbOperationError,
    },
    EffectiveUserLoaded {
        run_id: u64,
        effective_user: Option<String>,
    },
    TableDetailLoaded {
        dsn: String,
        run_id: u64,
        outcome: Result<Box<Table>, DbOperationError>,
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
    StartErPrefetchAll,
    StartErPrefetchScoped {
        tables: Vec<String>,
    },
    StartCompletionPrefetch {
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
    SqlModalNewLine,
    SqlModalInsertTab,
    SqlModalSubmit,
    SqlModalClear,
    SqlModalCancelConfirm,
    SqlModalConfirmExecute,
    SqlModalNextTab,
    SqlModalPrevTab,
    CompletionRequest,
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
        database_type: DatabaseType,
        database_generation: u64,
        run_id: u64,
        query: String,
        plan_text: String,
        is_analyze: bool,
        execution_time_ms: u64,
    },
    ExplainFailed {
        database_generation: u64,
        run_id: u64,
        error: DbOperationError,
        is_analyze: bool,
    },
    CompareEditQuery,

    // Query results
    ExecutePreview(TableTarget),
    QueryCompleted {
        run_id: u64,
        result: Arc<QueryResult>,
        context: QueryCompletionContext,
    },
    QueryFailed {
        run_id: u64,
        error: DbOperationError,
        context: QueryFailureContext,
    },
    RevealPendingPreview {
        generation: u64,
    },
    ExecuteWriteSucceeded {
        run_id: u64,
        affected_rows: usize,
        diagnostics: Vec<DatabaseDiagnostic>,
    },
    ExecuteWriteFailed {
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
    ResultRowYankOperatorPending,
    ResultRowYank,
    DdlYank,
    ResultDeleteOperatorPending,
    StageRowForDelete,
    UnstageLastStagedRow,
    ClearStagedDeletes,
    ResultEnterCellEdit,
    ResultOpenCellDetail,
    ResultCancelCellEdit,
    ResultDiscardCellEdit,
    SubmitCellEditWrite,
    CopySucceeded(CopyTarget),
    CopyFailed(ClipboardError),
    OpenFolderFailed(Arc<std::io::Error>),
    ToggleFocus,
    ToggleReadOnly,

    // Query history
    QueryHistoryLoaded(QueryHistoryScope, Vec<QueryHistoryEntry>),
    QueryHistoryLoadFailed(QueryHistoryScope, String),
    QueryHistoryConfirmSelection,

    // CSV export
    RequestCsvExport,
    CsvExportSucceeded {
        run_id: u64,
        path: String,
        row_count: Option<usize>,
    },
    CsvExportFailed {
        run_id: u64,
        error: DbOperationError,
    },

    // JSON and cell detail
    DetailSearch {
        target: DetailTarget,
        step: SearchStep,
    },
    DetailYankAll(DetailTarget),
    JsonEnterEdit,
    JsonAppendInsert,
    JsonExitEdit,

    // Row Detail
    RowDetailYank,
    RowDetailYankJson,

    // ER diagrams
    ErToggleSelection,
    ErSelectAll,
    ErConfirmSelection,
    ErOpenDiagram,
    ErGenerateFromCache,
    SmartErRefreshFetched(SmartErRefreshFetched),
    SmartErRefreshFailed(SmartErRefreshError),
    ErDiagramOpened(ErDiagramInfo),
    ErDiagramFailed {
        run_id: u64,
        error: String,
    },
    ErLogWriteFailed(String),
}

impl Action {
    pub fn text_edit(&self) -> Option<(InputTarget, TextEdit)> {
        let (target, edit) = match *self {
            Self::TextInput { target, ch } => (target, TextEdit::Insert(ch)),
            Self::TextBackspace { target } => (target, TextEdit::Backspace),
            Self::TextDelete { target } => (target, TextEdit::Delete),
            Self::TextKill { target, direction } => (target, TextEdit::Kill(direction)),
            Self::TextYank { target } => (target, TextEdit::Yank),
            Self::TextMoveCursor { target, direction } => (target, TextEdit::MoveCursor(direction)),
            _ => return None,
        };
        Some((target, edit))
    }

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

        if let Some((target, edit)) = self.text_edit() {
            return text_feature_requirement(target, edit);
        }

        match self {
            Self::OpenModal(ModalKind::ErTablePicker)
            | Self::ErToggleSelection
            | Self::ErSelectAll
            | Self::ErConfirmSelection
            | Self::ErOpenDiagram
            | Self::ErGenerateFromCache
            | Self::SmartErRefreshFetched(_)
            | Self::SmartErRefreshFailed(_)
            | Self::ErDiagramOpened(_)
            | Self::ErDiagramFailed { .. }
            | Self::ErLogWriteFailed(_)
            | Self::ListSelect {
                target: ListTarget::ErTablePicker,
                ..
            } => ErDiagram,
            Self::OpenModal(ModalKind::SqliteDiagnostics)
            | Self::RunSqliteDiagnosticsQuickCheck
            | Self::SqliteDiagnosticsCoreLoaded { .. }
            | Self::SqliteDiagnosticsQuickCheckLoaded { .. }
            | Self::Scroll {
                target: ScrollTarget::SqliteDiagnostics,
                ..
            } => SqliteDiagnostics,
            Self::OpenModal(ModalKind::JsonDetail)
            | Self::DetailYankAll(DetailTarget::JsonDetail)
            | Self::CopySucceeded(CopyTarget::Panel(FlashId::JsonDetail))
            | Self::DetailSearch {
                target: DetailTarget::JsonDetail,
                ..
            } => JsonDocumentDetail,
            Self::JsonEnterEdit | Self::JsonAppendInsert | Self::JsonExitEdit => JsonDocumentEdit,
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
            Self::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Exit,
            } if state.input_mode() == InputMode::JsonDetail
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

fn text_feature_requirement(target: InputTarget, edit: TextEdit) -> FeatureRequirement {
    match target {
        InputTarget::ErFilter => FeatureRequirement::ErDiagram,
        InputTarget::JsonSearch => FeatureRequirement::JsonDocumentDetail,
        InputTarget::JsonEdit if matches!(edit, TextEdit::MoveCursor(_)) => {
            FeatureRequirement::JsonDocumentDetail
        }
        InputTarget::JsonEdit => FeatureRequirement::JsonDocumentEdit,
        InputTarget::SqlModalAnalyzeHighRisk => FeatureRequirement::ExplainAnalyze,
        InputTarget::SqlModal
        | InputTarget::SqlModalHighRisk
        | InputTarget::ResultCellEdit
        | InputTarget::ConnectionSetup
        | InputTarget::CommandLine
        | InputTarget::Filter
        | InputTarget::SettingsErBrowser
        | InputTarget::QueryHistoryFilter
        | InputTarget::CellDetailSearch
        | InputTarget::HelpFilter => FeatureRequirement::None,
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
            Action::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Exit,
            }
            .feature_requirement_for_state(&search_state),
            FeatureRequirement::None
        );

        let normal_state = AppState::new("test".to_string());
        assert_eq!(
            Action::JsonExitEdit.feature_requirement_for_state(&normal_state),
            FeatureRequirement::JsonDocumentEdit
        );
    }

    mod feature_requirement_by_action_shape {
        use super::*;

        const TEXT_REQUIREMENTS: [(InputTarget, FeatureRequirement); 14] = [
            (InputTarget::SqlModal, FeatureRequirement::None),
            (InputTarget::SqlModalHighRisk, FeatureRequirement::None),
            (
                InputTarget::SqlModalAnalyzeHighRisk,
                FeatureRequirement::ExplainAnalyze,
            ),
            (InputTarget::ResultCellEdit, FeatureRequirement::None),
            (InputTarget::ConnectionSetup, FeatureRequirement::None),
            (InputTarget::CommandLine, FeatureRequirement::None),
            (InputTarget::Filter, FeatureRequirement::None),
            (InputTarget::ErFilter, FeatureRequirement::ErDiagram),
            (InputTarget::SettingsErBrowser, FeatureRequirement::None),
            (InputTarget::QueryHistoryFilter, FeatureRequirement::None),
            (InputTarget::JsonEdit, FeatureRequirement::JsonDocumentEdit),
            (
                InputTarget::JsonSearch,
                FeatureRequirement::JsonDocumentDetail,
            ),
            (InputTarget::CellDetailSearch, FeatureRequirement::None),
            (InputTarget::HelpFilter, FeatureRequirement::None),
        ];

        const JSON_EDIT_MOVE_CURSOR_REQUIREMENT: FeatureRequirement =
            FeatureRequirement::JsonDocumentDetail;

        fn text_actions(target: InputTarget) -> [(&'static str, Action); 6] {
            [
                ("Insert", Action::TextInput { target, ch: 'x' }),
                ("Backspace", Action::TextBackspace { target }),
                ("Delete", Action::TextDelete { target }),
                (
                    "Kill",
                    Action::TextKill {
                        target,
                        direction: TextKillDirection::ToLineEnd,
                    },
                ),
                ("Yank", Action::TextYank { target }),
                (
                    "MoveCursor",
                    Action::TextMoveCursor {
                        target,
                        direction: CursorMove::Left,
                    },
                ),
            ]
        }

        fn stateless_actions() -> Vec<(&'static str, Action)> {
            vec![
                (
                    "copied SqlModal",
                    Action::CopySucceeded(CopyTarget::Panel(FlashId::SqlModal)),
                ),
                (
                    "copied Ddl",
                    Action::CopySucceeded(CopyTarget::Panel(FlashId::Ddl)),
                ),
                (
                    "copied JsonDetail",
                    Action::CopySucceeded(CopyTarget::Panel(FlashId::JsonDetail)),
                ),
                (
                    "copied CellDetail",
                    Action::CopySucceeded(CopyTarget::Panel(FlashId::CellDetail)),
                ),
                (
                    "copied RowDetail",
                    Action::CopySucceeded(CopyTarget::Panel(FlashId::RowDetail)),
                ),
                (
                    "copied ResultCell",
                    Action::CopySucceeded(CopyTarget::ResultCell { row: 0, col: 0 }),
                ),
                (
                    "copied ResultRow",
                    Action::CopySucceeded(CopyTarget::ResultRow { row: 0 }),
                ),
                (
                    "copied ConnectionError",
                    Action::CopySucceeded(CopyTarget::ConnectionError),
                ),
                (
                    "search JsonDetail Enter",
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Enter,
                    },
                ),
                (
                    "search JsonDetail Exit",
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Exit,
                    },
                ),
                (
                    "search JsonDetail Next",
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Next,
                    },
                ),
                (
                    "search JsonDetail Prev",
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Prev,
                    },
                ),
                (
                    "search JsonDetail Submit",
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Submit,
                    },
                ),
                (
                    "search CellDetail Enter",
                    Action::DetailSearch {
                        target: DetailTarget::CellDetail,
                        step: SearchStep::Enter,
                    },
                ),
                (
                    "search CellDetail Exit",
                    Action::DetailSearch {
                        target: DetailTarget::CellDetail,
                        step: SearchStep::Exit,
                    },
                ),
                (
                    "search CellDetail Next",
                    Action::DetailSearch {
                        target: DetailTarget::CellDetail,
                        step: SearchStep::Next,
                    },
                ),
                (
                    "search CellDetail Prev",
                    Action::DetailSearch {
                        target: DetailTarget::CellDetail,
                        step: SearchStep::Prev,
                    },
                ),
                (
                    "search CellDetail Submit",
                    Action::DetailSearch {
                        target: DetailTarget::CellDetail,
                        step: SearchStep::Submit,
                    },
                ),
                (
                    "yank all JsonDetail",
                    Action::DetailYankAll(DetailTarget::JsonDetail),
                ),
                (
                    "yank all CellDetail",
                    Action::DetailYankAll(DetailTarget::CellDetail),
                ),
                ("toggle Help", Action::ToggleHelp),
            ]
        }

        fn state_in(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        fn stateful_actions() -> Vec<(&'static str, AppState, Action)> {
            let mut searching = state_in(InputMode::JsonDetail);
            searching.json_detail.enter_search();
            vec![
                (
                    "search JsonDetail Exit while searching",
                    searching,
                    Action::DetailSearch {
                        target: DetailTarget::JsonDetail,
                        step: SearchStep::Exit,
                    },
                ),
                (
                    "paste in JsonDetail",
                    state_in(InputMode::JsonDetail),
                    Action::Paste("x".to_string()),
                ),
                (
                    "paste in JsonEdit",
                    state_in(InputMode::JsonEdit),
                    Action::Paste("x".to_string()),
                ),
                (
                    "paste in ErTablePicker",
                    state_in(InputMode::ErTablePicker),
                    Action::Paste("x".to_string()),
                ),
                (
                    "paste in CellDetail",
                    state_in(InputMode::CellDetail),
                    Action::Paste("x".to_string()),
                ),
            ]
        }

        fn report() -> String {
            let normal_state = AppState::new("test".to_string());
            let stateless = stateless_actions().into_iter().map(|(label, action)| {
                format!(
                    "{label} => {:?}",
                    action.feature_requirement_for_state(&normal_state)
                )
            });
            let stateful = stateful_actions()
                .into_iter()
                .map(|(label, state, action)| {
                    format!(
                        "{label} => {:?}",
                        action.feature_requirement_for_state(&state)
                    )
                });
            stateless.chain(stateful).collect::<Vec<_>>().join("\n")
        }

        #[test]
        fn text_actions_keep_requirements_per_target() {
            let state = AppState::new("test".to_string());
            for (target, requirement) in TEXT_REQUIREMENTS {
                for (op, action) in text_actions(target) {
                    let expected = if target == InputTarget::JsonEdit
                        && matches!(action, Action::TextMoveCursor { .. })
                    {
                        JSON_EDIT_MOVE_CURSOR_REQUIREMENT
                    } else {
                        requirement
                    };

                    assert_eq!(
                        action.feature_requirement_for_state(&state),
                        expected,
                        "text {target:?} {op}"
                    );
                }
            }
        }

        #[test]
        fn keeps_requirements_for_each_shape() {
            let report = report();

            assert_eq!(report, EXPECTED_REPORT, "\n{report}\n");
        }

        const EXPECTED_REPORT: &str = "\
copied SqlModal => None\n\
copied Ddl => None\n\
copied JsonDetail => JsonDocumentDetail\n\
copied CellDetail => None\n\
copied RowDetail => None\n\
copied ResultCell => None\n\
copied ResultRow => None\n\
copied ConnectionError => None\n\
search JsonDetail Enter => JsonDocumentDetail\n\
search JsonDetail Exit => JsonDocumentDetail\n\
search JsonDetail Next => JsonDocumentDetail\n\
search JsonDetail Prev => JsonDocumentDetail\n\
search JsonDetail Submit => JsonDocumentDetail\n\
search CellDetail Enter => None\n\
search CellDetail Exit => None\n\
search CellDetail Next => None\n\
search CellDetail Prev => None\n\
search CellDetail Submit => None\n\
yank all JsonDetail => JsonDocumentDetail\n\
yank all CellDetail => None\n\
toggle Help => None\n\
search JsonDetail Exit while searching => None\n\
paste in JsonDetail => JsonDocumentDetail\n\
paste in JsonEdit => JsonDocumentEdit\n\
paste in ErTablePicker => ErDiagram\n\
paste in CellDetail => None";
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
