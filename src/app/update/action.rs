use std::sync::Arc;

use crate::domain::connection::{
    ConnectionEndpoint, ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType,
    ServiceEntry,
};
use crate::domain::query_history::{QueryHistoryEntry, QueryHistoryScope};
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::Prefix;
use crate::policy::FeatureRequirement;
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

impl CursorPosition {
    pub(crate) fn viewport_item(self, offset: usize, visible: usize, total: usize) -> usize {
        let displayed = visible.min(total.saturating_sub(offset));
        match self {
            Self::Top => offset,
            Self::Center => offset + displayed / 2,
            Self::Bottom => offset + displayed.saturating_sub(1),
        }
    }

    pub(crate) fn scroll_offset(
        self,
        selected: usize,
        visible: usize,
        max: usize,
    ) -> Option<usize> {
        if visible == 0 {
            return None;
        }
        let offset = match self {
            Self::Center => selected.saturating_sub(visible / 2),
            Self::Top => selected,
            Self::Bottom => selected.saturating_sub(visible - 1),
        };
        Some(offset.min(max))
    }
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

impl ModalKind {
    pub fn feature_requirement(self) -> FeatureRequirement {
        match self {
            Self::ErTablePicker => FeatureRequirement::ErDiagram,
            Self::SqliteDiagnostics => FeatureRequirement::SqliteDiagnostics,
            Self::JsonDetail => FeatureRequirement::JsonDocumentDetail,
            _ => FeatureRequirement::None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshSnapshot {
    pub new_metadata: Arc<DatabaseMetadata>,
    pub signature_snapshot: Arc<TableSignatureSnapshot>,
}

#[derive(Debug, Clone)]
pub struct SmartErRefreshError {
    pub error: DbOperationError,
    pub new_metadata: Option<Arc<DatabaseMetadata>>,
}

#[derive(Debug, Clone)]
pub struct ErDiagramInfo {
    pub path: String,
    pub table_count: usize,
    pub total_tables: usize,
}

#[derive(Debug, Clone)]
pub struct SavedConnection {
    pub target: ConnectionTarget,
    pub mysql_lower_case_table_names: Option<u8>,
    pub metadata: Option<Arc<DatabaseMetadata>>,
}

#[derive(Debug, Clone)]
pub struct ExplainPlan {
    pub database_type: DatabaseType,
    pub query: String,
    pub plan_text: String,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub affected_rows: usize,
    pub diagnostics: Vec<DatabaseDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct CsvExport {
    pub path: String,
    pub row_count: usize,
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
pub struct ConnectionTarget {
    pub id: ConnectionId,
    pub name: String,
    pub endpoint: Arc<ConnectionEndpoint>,
}

impl ConnectionTarget {
    pub fn from_profile(profile: &ConnectionProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.display_name().to_string(),
            endpoint: Arc::new(ConnectionEndpoint::from(&profile.config)),
        }
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
pub enum QueryContext {
    Adhoc,
    Preview { generation: u64, target_page: usize },
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
    Paste {
        target: InputTarget,
        text: String,
    },
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
    ConnectionSaved {
        run_id: u64,
        outcome: Result<SavedConnection, ConnectionSaveError>,
    },
    MySqlConnectionProbed {
        target: ConnectionTarget,
        run_id: u64,
        outcome: Result<u8, DbOperationError>,
    },
    ConnectionEditLoaded(Result<Box<ConnectionProfile>, String>),
    CloseConnectionError,
    ToggleConnectionErrorDetails,
    CopyConnectionError,
    ReenterConnectionSetup,
    RetryConnection,
    RequestDeleteSelectedConnection,
    ConnectionDeleted(Result<ConnectionId, String>),
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
    SettingsSaved(Result<AppSettings, String>),

    // Database structure
    ReloadMetadata,
    MetadataLoaded {
        run_id: u64,
        outcome: Result<Arc<DatabaseMetadata>, DbOperationError>,
    },
    EffectiveUserLoaded {
        run_id: u64,
        effective_user: Option<String>,
    },
    TableDetailLoaded {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        outcome: Result<Box<Table>, DbOperationError>,
        generation: u64,
    },
    TableDetailCached {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        schema: String,
        table: String,
        outcome: Result<Box<Table>, DbOperationError>,
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
        database_generation: u64,
        run_id: u64,
        is_analyze: bool,
        outcome: Result<ExplainPlan, DbOperationError>,
    },
    CompareEditQuery,

    // Query results
    QueryCompleted {
        run_id: u64,
        context: QueryContext,
        outcome: Result<Arc<QueryResult>, DbOperationError>,
    },
    WriteExecuted {
        run_id: u64,
        outcome: Result<WriteOutcome, DbOperationError>,
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
    Copied {
        target: CopyTarget,
        outcome: Result<(), ClipboardError>,
    },
    OpenFolderFailed(Arc<std::io::Error>),
    ToggleFocus,
    ToggleReadOnly,

    // Query history
    QueryHistoryLoaded(QueryHistoryScope, Result<Vec<QueryHistoryEntry>, String>),
    QueryHistoryConfirmSelection,

    // CSV export
    RequestCsvExport,
    CsvExported {
        run_id: u64,
        outcome: Result<CsvExport, DbOperationError>,
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
    SmartErRefreshFetched {
        endpoint: Arc<ConnectionEndpoint>,
        run_id: u64,
        outcome: Result<SmartErRefreshSnapshot, SmartErRefreshError>,
    },
    ErDiagramOpened {
        run_id: u64,
        outcome: Result<ErDiagramInfo, String>,
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
        if let Self::Paste { target, .. } = *self {
            return text_feature_requirement(target, TextEdit::Insert(' '));
        }

        match self {
            Self::OpenModal(kind) => kind.feature_requirement(),
            Self::ErToggleSelection
            | Self::ErSelectAll
            | Self::ErConfirmSelection
            | Self::SmartErRefreshFetched { .. }
            | Self::ErDiagramOpened { .. }
            | Self::ErLogWriteFailed(_)
            | Self::ListSelect {
                target: ListTarget::ErTablePicker,
                ..
            } => ErDiagram,
            Self::RunSqliteDiagnosticsQuickCheck
            | Self::SqliteDiagnosticsCoreLoaded { .. }
            | Self::SqliteDiagnosticsQuickCheckLoaded { .. }
            | Self::Scroll {
                target: ScrollTarget::SqliteDiagnostics,
                ..
            } => SqliteDiagnostics,
            Self::DetailYankAll(DetailTarget::JsonDetail)
            | Self::Copied {
                target: CopyTarget::Panel(FlashId::JsonDetail),
                ..
            }
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
        self.feature_requirement_for_context(
            state.input_mode(),
            state.json_detail.search().is_active(),
        )
    }

    pub(crate) fn feature_requirement_for_context(
        &self,
        mode: InputMode,
        json_searching: bool,
    ) -> FeatureRequirement {
        match self {
            Self::JsonExitEdit if mode == InputMode::JsonEdit => FeatureRequirement::None,
            Self::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Exit,
            } if mode == InputMode::JsonDetail && json_searching => FeatureRequirement::None,
            Self::BeginKeySequence(Prefix::G)
                if matches!(mode, InputMode::JsonDetail | InputMode::JsonEdit) =>
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
    use crate::domain::connection::{MySqlConnectionConfig, MySqlSslMode};
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
        let target = ConnectionTarget::from_profile(&profile);

        assert_eq!(target.id, profile.id);
        assert_eq!(*target.endpoint, ConnectionEndpoint::from(&profile.config));
        assert_eq!(target.name, profile.display_name());
        assert_eq!(target.endpoint.engine(), profile.database_type());
        assert_eq!(target.endpoint.selected_database(), Some("app"));
    }

    #[test]
    fn connection_target_debug_masks_mysql_password() {
        let target = ConnectionTarget {
            id: ConnectionId::from_string("mysql"),
            name: "MySQL".to_string(),
            endpoint: Arc::new(ConnectionEndpoint::MySQL(MySqlConnectionConfig::new(
                "localhost",
                3306,
                Some("app".to_string()),
                "user",
                "secret",
                MySqlSslMode::Required,
            ))),
        };

        let debug = format!("{target:?}");

        assert!(!debug.contains("secret"));
        assert!(debug.contains(r#"password: "****""#));
        assert!(debug.contains(r#"host: "localhost""#));
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
                database_generation: 0,
                run_id: 1,
                is_analyze: true,
                outcome: Ok(ExplainPlan {
                    database_type: DatabaseType::PostgreSQL,
                    query: "SELECT 1".to_string(),
                    plan_text: "plan".to_string(),
                    execution_time_ms: 1
                })
            }
            .feature_requirement(),
            FeatureRequirement::ExplainAnalyze
        );
        assert_eq!(
            Action::ExplainCompleted {
                database_generation: 0,
                run_id: 1,
                is_analyze: false,
                outcome: Err(DbOperationError::QueryFailed("error".to_string()))
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

        fn text_actions(target: InputTarget) -> [(&'static str, Action); 7] {
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
                (
                    "Paste",
                    Action::Paste {
                        target,
                        text: "x".to_string(),
                    },
                ),
            ]
        }

        fn stateless_actions() -> Vec<(&'static str, Action)> {
            vec![
                (
                    "copied SqlModal",
                    Action::Copied {
                        target: CopyTarget::Panel(FlashId::SqlModal),
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied Ddl",
                    Action::Copied {
                        target: CopyTarget::Panel(FlashId::Ddl),
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied JsonDetail",
                    Action::Copied {
                        target: CopyTarget::Panel(FlashId::JsonDetail),
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied CellDetail",
                    Action::Copied {
                        target: CopyTarget::Panel(FlashId::CellDetail),
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied RowDetail",
                    Action::Copied {
                        target: CopyTarget::Panel(FlashId::RowDetail),
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied ResultCell",
                    Action::Copied {
                        target: CopyTarget::ResultCell { row: 0, col: 0 },
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied ResultRow",
                    Action::Copied {
                        target: CopyTarget::ResultRow { row: 0 },
                        outcome: Ok(()),
                    },
                ),
                (
                    "copied ConnectionError",
                    Action::Copied {
                        target: CopyTarget::ConnectionError,
                        outcome: Ok(()),
                    },
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
            vec![(
                "search JsonDetail Exit while searching",
                searching,
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Exit,
                },
            )]
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
search JsonDetail Exit while searching => None";
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
