use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::connection::list::is_service_selected;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::{ErBrowserChoice, KeymapPreset, SettingsSection};
use crate::model::shared::ui_state::ResultNavMode;
use crate::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyContext {
    pub(super) preset: KeymapPreset,
    mode: InputMode,
    json_mode: JsonDetailMode,
    policy: FeaturePolicy,
    scope: Scope,
    pending_prefix: Option<Prefix>,
    history_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Diagnostics {
        can_run: bool,
    },
    CellDetail {
        searching: bool,
    },
    Help {
        editing: bool,
    },
    Confirm {
        blocked: bool,
    },
    ConnectionSelector {
        service_selected: bool,
    },
    ConnectionError {
        can_retry: bool,
    },
    ConnectionSetup {
        dropdown_open: bool,
        dropdown_field: bool,
        text_focused: bool,
        editing: bool,
    },
    Settings {
        editing: bool,
        custom_selected: bool,
    },
    SqlConfirmation {
        kind: SqlConfirmation,
        matched: bool,
    },
    SqlEditing {
        completion_visible: bool,
    },
    SqlNormal {
        tab: SqlModalTab,
        multiple_tabs: bool,
        compare_can_yank: bool,
    },
    Browse(BrowseScope),
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Browse bindings depend on independent state flags."
)]
struct BrowseScope {
    focused: bool,
    read_only: bool,
    csv_available: bool,
    pane: BrowsePane,
    connection_error: bool,
    inspector_ddl: bool,
    staged_delete: bool,
    draft: bool,
    yank_pending: bool,
    delete_pending: bool,
    selected_row: bool,
    selected_cell: bool,
    connected: bool,
    can_edit_cell: bool,
    can_write: bool,
    can_paginate: bool,
    multiple_inspector_tabs: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum BrowsePane {
    #[default]
    Explorer,
    Inspector,
    ResultScroll,
    ResultCell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SqlConfirmation {
    High,
    Risk,
    AnalyzeHigh,
    AnalyzeRisk,
}

impl KeyContext {
    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub(crate) fn preset(&self) -> KeymapPreset {
        self.preset
    }

    pub(crate) fn policy(&self) -> &FeaturePolicy {
        &self.policy
    }

    pub fn label(&self) -> &'static str {
        match self.mode {
            InputMode::Normal => match self.browse_pane() {
                BrowsePane::Explorer => "Explorer Pane",
                BrowsePane::Inspector => "Inspector Pane",
                BrowsePane::ResultScroll | BrowsePane::ResultCell => "Result Pane",
            },
            InputMode::CommandLine => "Command Line",
            InputMode::CellEdit => "Cell Edit",
            InputMode::TablePicker => "Table Picker",
            InputMode::CommandPalette => "Command Palette",
            InputMode::Settings => "Settings",
            InputMode::Help => "Help",
            InputMode::SqlModal => match self.scope {
                Scope::SqlEditing { .. } => "SQL Editor Insert",
                Scope::SqlConfirmation { .. } => "SQL Editor Confirm",
                Scope::SqlNormal {
                    tab: SqlModalTab::Sql,
                    ..
                } => "SQL Editor",
                Scope::SqlNormal {
                    tab: SqlModalTab::Plan,
                    ..
                } => "SQL Editor Plan",
                Scope::SqlNormal {
                    tab: SqlModalTab::Compare,
                    ..
                } => "SQL Editor Compare",
                _ => "SQL Editor Running",
            },
            InputMode::ConnectionSetup => "Connection Setup",
            InputMode::ConnectionError => "Connection Error",
            InputMode::SqliteDiagnostics => "SQLite Diagnostics",
            InputMode::ConfirmDialog => "Confirm Dialog",
            InputMode::ConnectionSelector => "Connection Selector",
            InputMode::ErTablePicker => "ER Table Picker",
            InputMode::QueryHistoryPicker => "Query History Picker",
            InputMode::JsonDetail => match self.json_mode {
                JsonDetailMode::Viewing => "JSON Detail",
                JsonDetailMode::Searching => "JSON Search",
                JsonDetailMode::Editing => "JSON Edit",
            },
            InputMode::JsonEdit => "JSON Edit",
            InputMode::CellDetail if self.is_cell_detail_searching() => "Cell Detail Search",
            InputMode::CellDetail => "Cell Detail",
            InputMode::RowDetail => "Row Detail",
        }
    }

    pub(super) fn is_sql_editing(&self) -> bool {
        matches!(self.scope, Scope::SqlEditing { .. })
    }

    pub(super) fn json_mode(&self) -> JsonDetailMode {
        self.json_mode
    }
    pub fn from_state(state: &AppState) -> Self {
        Self {
            preset: state.settings.saved_keymap_preset(),
            mode: state.input_mode(),
            json_mode: state.json_detail.mode(),
            policy: FeaturePolicy::new(&state.session.active_engine_feature_profile()),
            pending_prefix: state.ui.key_sequence().pending_prefix(),
            history_available: state.can_open_query_history(),
            scope: match state.input_mode() {
                InputMode::Normal => browse_scope(state),
                InputMode::SqliteDiagnostics => Scope::Diagnostics {
                    can_run: state.sqlite_diagnostics.can_run_quick_check(),
                },
                InputMode::CellDetail => Scope::CellDetail {
                    searching: state.cell_detail.search().is_active(),
                },
                InputMode::Help => Scope::Help {
                    editing: state.ui.help().mode() == HelpMode::EditingFilter,
                },
                InputMode::ConfirmDialog => Scope::Confirm {
                    blocked: state
                        .result_interaction
                        .pending_write_preview()
                        .is_some_and(|preview| preview.guardrail.blocked),
                },
                InputMode::ConnectionSelector => Scope::ConnectionSelector {
                    service_selected: is_service_selected(
                        state.connection_list_items(),
                        state.ui.connection_list_selected(),
                    ),
                },
                InputMode::ConnectionError => Scope::ConnectionError {
                    can_retry: state.can_retry_connection_error(),
                },
                InputMode::ConnectionSetup => Scope::ConnectionSetup {
                    dropdown_open: state.connection_setup.has_open_dropdown(),
                    dropdown_field: is_dropdown_field(state.connection_setup.focused_field()),
                    text_focused: state.connection_setup.focused_input().is_some(),
                    editing: state.connection_setup.is_edit_mode(),
                },
                InputMode::Settings => Scope::Settings {
                    editing: state.settings.is_editing_custom_er_browser(),
                    custom_selected: state.settings.section() == SettingsSection::ErDiagram
                        && state.settings.selected_er_browser_choice() == ErBrowserChoice::Custom,
                },
                InputMode::SqlModal
                    if matches!(state.sql_modal.status(), SqlModalStatus::Editing) =>
                {
                    Scope::SqlEditing {
                        completion_visible: state.sql_modal.completion().visible
                            && !state.sql_modal.completion().candidates.is_empty(),
                    }
                }
                InputMode::SqlModal
                    if matches!(
                        state.sql_modal.status(),
                        SqlModalStatus::Normal
                            | SqlModalStatus::Success(_)
                            | SqlModalStatus::Error(_)
                    ) =>
                {
                    let profile = state.session.active_engine_feature_profile();
                    Scope::SqlNormal {
                        tab: profile.normalize_sql_modal_tab(state.sql_modal.active_tab()),
                        multiple_tabs: profile.supported_sql_modal_tabs().len() > 1,
                        compare_can_yank: state.explain.can_yank_compare(),
                    }
                }
                InputMode::SqlModal => {
                    sql_confirmation_scope(state.sql_modal.status()).unwrap_or(Scope::Other)
                }
                _ => Scope::Other,
            },
        }
    }

    pub(crate) fn for_scope(mode: InputMode, preset: KeymapPreset, policy: &FeaturePolicy) -> Self {
        Self {
            preset,
            mode,
            json_mode: JsonDetailMode::Viewing,
            policy: policy.clone(),
            pending_prefix: None,
            history_available: true,
            scope: match mode {
                InputMode::SqliteDiagnostics => Scope::Diagnostics { can_run: true },
                InputMode::CellDetail => Scope::CellDetail { searching: false },
                InputMode::Help => Scope::Help { editing: false },
                InputMode::ConfirmDialog => Scope::Confirm { blocked: false },
                InputMode::ConnectionSelector => Scope::ConnectionSelector {
                    service_selected: false,
                },
                InputMode::ConnectionError => Scope::ConnectionError { can_retry: false },
                InputMode::ConnectionSetup => Scope::ConnectionSetup {
                    dropdown_open: false,
                    dropdown_field: true,
                    text_focused: false,
                    editing: false,
                },
                InputMode::Settings => Scope::Settings {
                    editing: false,
                    custom_selected: true,
                },
                _ => Scope::Other,
            },
        }
    }

    pub(super) fn with_browse_reference(
        mut self,
        pane: BrowsePane,
        staged: bool,
        draft: bool,
    ) -> Self {
        self.mode = InputMode::Normal;
        self.history_available = true;
        self.pending_prefix = None;
        self.scope = Scope::Browse(BrowseScope {
            focused: false,
            read_only: false,
            csv_available: true,
            pane,
            connection_error: false,
            inspector_ddl: true,
            staged_delete: staged,
            draft,
            yank_pending: false,
            delete_pending: false,
            selected_row: true,
            selected_cell: true,
            connected: true,
            can_edit_cell: true,
            can_write: true,
            can_paginate: true,
            multiple_inspector_tabs: true,
        });
        self
    }

    pub(super) fn with_browse_reference_error(mut self) -> Self {
        if let Scope::Browse(ref mut scope) = self.scope {
            scope.connection_error = true;
        }
        self
    }

    pub(super) fn with_browse_reference_focus_read_only(mut self) -> Self {
        if let Scope::Browse(ref mut scope) = self.scope {
            scope.focused = true;
            scope.read_only = true;
        }
        self
    }

    pub fn for_browse_state(state: &AppState) -> Self {
        let mut context = Self::from_state(state);
        if context.mode != InputMode::Normal {
            context.mode = InputMode::Normal;
            context.scope = browse_scope(state);
        }
        context
    }

    fn browse(&self) -> BrowseScope {
        match self.scope {
            Scope::Browse(scope) => scope,
            _ => BrowseScope::default(),
        }
    }

    pub(super) fn browse_pane(&self) -> BrowsePane {
        self.browse().pane
    }

    pub(super) fn browse_focused(&self) -> bool {
        self.browse().focused
    }

    pub(super) fn browse_read_only(&self) -> bool {
        self.browse().read_only
    }

    pub(super) fn browse_csv_available(&self) -> bool {
        self.browse().csv_available
    }

    pub(super) fn browse_connection_error(&self) -> bool {
        self.browse().connection_error
    }

    pub(super) fn browse_inspector_ddl(&self) -> bool {
        self.browse().inspector_ddl
    }

    pub(super) fn browse_staged_delete(&self) -> bool {
        self.browse().staged_delete
    }

    pub(super) fn browse_draft(&self) -> bool {
        self.browse().draft
    }

    pub(super) fn browse_yank_pending(&self) -> bool {
        self.browse().yank_pending
    }

    pub(super) fn browse_delete_pending(&self) -> bool {
        self.browse().delete_pending
    }

    pub(super) fn browse_selected_row(&self) -> bool {
        self.browse().selected_row
    }

    pub(super) fn browse_selected_cell(&self) -> bool {
        self.browse().selected_cell
    }

    pub(super) fn browse_connected(&self) -> bool {
        self.browse().connected
    }

    pub(super) fn browse_can_edit_cell(&self) -> bool {
        self.browse().can_edit_cell
    }

    pub(super) fn browse_can_write(&self) -> bool {
        self.browse().can_write
    }

    pub(super) fn browse_can_paginate(&self) -> bool {
        self.browse().can_paginate
    }

    pub(super) fn browse_multiple_inspector_tabs(&self) -> bool {
        self.browse().multiple_inspector_tabs
    }

    pub(super) fn allows(&self, action: &Action) -> bool {
        self.policy
            .is_enabled(action.feature_requirement_for_context(self.mode, self.json_mode))
    }

    pub(super) fn pending_prefix(&self) -> Option<Prefix> {
        self.pending_prefix
    }

    pub(super) fn supports_diagnostics(&self) -> bool {
        self.policy
            .is_enabled(FeatureRequirement::SqliteDiagnostics)
    }

    pub(super) fn supports_json_edit(&self) -> bool {
        self.policy.is_enabled(FeatureRequirement::JsonDocumentEdit)
    }

    pub(super) fn supports_json_detail(&self) -> bool {
        self.policy
            .is_enabled(FeatureRequirement::JsonDocumentDetail)
    }

    pub(super) fn sql_confirmation(&self) -> Option<SqlConfirmation> {
        match self.scope {
            Scope::SqlConfirmation { kind, .. } => Some(kind),
            _ => None,
        }
    }

    pub(super) fn is_sql_confirmation_matched(&self) -> bool {
        matches!(self.scope, Scope::SqlConfirmation { matched: true, .. })
    }

    pub(super) fn is_sql_completion_visible(&self) -> bool {
        matches!(
            self.scope,
            Scope::SqlEditing {
                completion_visible: true
            }
        )
    }

    pub(super) fn can_open_history(&self) -> bool {
        self.history_available && !self.is_sql_completion_visible()
    }

    pub(super) fn sql_tab(&self) -> Option<SqlModalTab> {
        match self.scope {
            Scope::SqlNormal { tab, .. } => Some(tab),
            _ => None,
        }
    }

    pub(super) fn sql_has_multiple_tabs(&self) -> bool {
        matches!(
            self.scope,
            Scope::SqlNormal {
                multiple_tabs: true,
                ..
            }
        )
    }

    pub(super) fn sql_compare_can_yank(&self) -> bool {
        matches!(
            self.scope,
            Scope::SqlNormal {
                compare_can_yank: true,
                ..
            }
        )
    }

    pub(super) fn cancel_unmatched_sql_prefix(&self) -> bool {
        matches!(
            self.scope,
            Scope::SqlNormal {
                tab: SqlModalTab::Plan | SqlModalTab::Compare,
                ..
            }
        )
    }

    pub(crate) fn with_sql_view(mut self, tab: SqlModalTab) -> Self {
        self.mode = InputMode::SqlModal;
        self.scope = Scope::SqlNormal {
            tab,
            multiple_tabs: self.policy.is_enabled(FeatureRequirement::Explain),
            compare_can_yank: true,
        };
        self
    }

    pub(crate) fn with_sql_editing(mut self, completion_visible: bool) -> Self {
        self.mode = InputMode::SqlModal;
        self.scope = Scope::SqlEditing { completion_visible };
        self
    }

    pub(super) fn with_sql_confirmation(mut self, kind: SqlConfirmation) -> Self {
        self.mode = InputMode::SqlModal;
        self.scope = Scope::SqlConfirmation {
            kind,
            matched: true,
        };
        self
    }

    pub(super) fn can_run_diagnostics(&self) -> bool {
        matches!(self.scope, Scope::Diagnostics { can_run: true })
    }

    pub(super) fn is_cell_detail_searching(&self) -> bool {
        matches!(self.scope, Scope::CellDetail { searching: true })
    }

    pub(super) fn is_help_editing(&self) -> bool {
        matches!(self.scope, Scope::Help { editing: true })
    }

    pub(super) fn is_confirm_blocked(&self) -> bool {
        matches!(self.scope, Scope::Confirm { blocked: true })
    }

    pub(super) fn is_service_selected(&self) -> bool {
        matches!(
            self.scope,
            Scope::ConnectionSelector {
                service_selected: true
            }
        )
    }

    pub(super) fn can_retry_connection(&self) -> bool {
        matches!(self.scope, Scope::ConnectionError { can_retry: true })
    }

    pub(super) fn is_setup_dropdown_open(&self) -> bool {
        matches!(
            self.scope,
            Scope::ConnectionSetup {
                dropdown_open: true,
                ..
            }
        )
    }

    pub(super) fn is_setup_dropdown_field(&self) -> bool {
        matches!(
            self.scope,
            Scope::ConnectionSetup {
                dropdown_field: true,
                ..
            }
        )
    }

    pub(super) fn is_setup_text_focused(&self) -> bool {
        matches!(
            self.scope,
            Scope::ConnectionSetup {
                text_focused: true,
                ..
            }
        )
    }

    pub(super) fn is_setup_editing(&self) -> bool {
        matches!(self.scope, Scope::ConnectionSetup { editing: true, .. })
    }

    pub(crate) fn with_connection_setup(
        mut self,
        field: ConnectionField,
        dropdown_open: bool,
    ) -> Self {
        let dropdown_field = is_dropdown_field(field);
        self.mode = InputMode::ConnectionSetup;
        self.scope = Scope::ConnectionSetup {
            dropdown_open,
            dropdown_field,
            text_focused: !dropdown_field,
            editing: false,
        };
        self
    }

    pub(super) fn is_settings_editing(&self) -> bool {
        matches!(self.scope, Scope::Settings { editing: true, .. })
    }

    pub(super) fn with_settings_editing(mut self) -> Self {
        self.mode = InputMode::Settings;
        self.scope = Scope::Settings {
            editing: true,
            custom_selected: true,
        };
        self
    }

    pub(super) fn with_help_editing(mut self) -> Self {
        self.mode = InputMode::Help;
        self.scope = Scope::Help { editing: true };
        self
    }

    pub(super) fn is_custom_browser_selected(&self) -> bool {
        matches!(
            self.scope,
            Scope::Settings {
                custom_selected: true,
                ..
            }
        )
    }

    pub(crate) fn with_cell_detail_searching(mut self, searching: bool) -> Self {
        self.mode = InputMode::CellDetail;
        self.scope = Scope::CellDetail { searching };
        self
    }

    pub(crate) fn with_json_mode(mut self, mode: JsonDetailMode) -> Self {
        self.mode = InputMode::JsonDetail;
        self.json_mode = mode;
        self
    }

    pub(crate) fn with_json_edit(mut self) -> Self {
        self.mode = InputMode::JsonEdit;
        self.json_mode = JsonDetailMode::Editing;
        self
    }
}

fn is_dropdown_field(field: ConnectionField) -> bool {
    matches!(
        field,
        ConnectionField::DatabaseType
            | ConnectionField::Transport
            | ConnectionField::SslMode
            | ConnectionField::CleartextAuth
    )
}

fn browse_scope(state: &AppState) -> Scope {
    let connected = state.session.endpoint().is_some()
        && state.session.pending_mysql_connection_probe().is_none();
    let pane = if state.ui.is_focus_mode() || state.ui.focused_pane() == FocusedPane::Result {
        if state.result_interaction.selection().mode() == ResultNavMode::CellActive {
            BrowsePane::ResultCell
        } else {
            BrowsePane::ResultScroll
        }
    } else if state.ui.focused_pane() == FocusedPane::Inspector {
        BrowsePane::Inspector
    } else {
        BrowsePane::Explorer
    };
    Scope::Browse(BrowseScope {
        focused: state.ui.is_focus_mode(),
        read_only: state.session.is_read_only(),
        csv_available: connected && state.can_request_csv_export(),
        pane,
        connection_error: state.connection_error.has_error(),
        inspector_ddl: state.ui.inspector_tab() == InspectorTab::Ddl,
        staged_delete: !state.result_interaction.staged_delete_rows().is_empty(),
        draft: state.result_interaction.cell_edit().has_pending_draft(),
        yank_pending: state.result_interaction.is_yank_operator_pending(),
        delete_pending: state.result_interaction.is_delete_operator_pending(),
        selected_row: state.result_interaction.selection().row().is_some(),
        selected_cell: state.result_interaction.selection().cell().is_some(),
        connected,
        can_edit_cell: state.can_edit_selected_cell(),
        can_write: state.can_write_visible_preview(),
        can_paginate: state.query.can_paginate_visible_result(),
        multiple_inspector_tabs: state
            .session
            .active_engine_feature_profile()
            .supported_inspector_tabs()
            .len()
            > 1,
    })
}

fn sql_confirmation_scope(status: &SqlModalStatus) -> Option<Scope> {
    let (kind, matched) = match status {
        SqlModalStatus::ConfirmingHigh {
            input, target_name, ..
        } => (
            SqlConfirmation::High,
            input.content() == target_name.as_str(),
        ),
        SqlModalStatus::ConfirmingRisk { .. } => (SqlConfirmation::Risk, true),
        SqlModalStatus::ConfirmingAnalyzeHigh {
            input, target_name, ..
        } => (
            SqlConfirmation::AnalyzeHigh,
            input.content() == target_name.as_str(),
        ),
        SqlModalStatus::ConfirmingAnalyzeRisk { .. } => (SqlConfirmation::AnalyzeRisk, true),
        _ => return None,
    };
    Some(Scope::SqlConfirmation { kind, matched })
}
