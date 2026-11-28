use crate::model::app_state::AppState;
use crate::model::connection::list::is_service_selected;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::KeymapPreset;
use crate::model::shared::ui_state::ResultNavMode;
use crate::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Key bindings depend on independent state flags."
)]
pub struct KeyContext {
    pub(super) preset: KeymapPreset,
    pub(super) mode: InputMode,
    pub(super) json_searching: bool,
    pub(super) policy: FeaturePolicy,
    pub(super) pending_prefix: Option<Prefix>,
    label: &'static str,
    pub(super) is_sql_editing: bool,
    pub(super) browse_pane: BrowsePane,
    pub(super) browse_focused: bool,
    pub(super) browse_read_only: bool,
    pub(super) browse_csv_available: bool,
    pub(super) browse_connection_error: bool,
    pub(super) browse_inspector_ddl: bool,
    pub(super) browse_staged_delete: bool,
    pub(super) browse_draft: bool,
    pub(super) browse_yank_pending: bool,
    pub(super) browse_delete_pending: bool,
    pub(super) browse_selected_row: bool,
    pub(super) browse_selected_cell: bool,
    pub(super) browse_connected: bool,
    pub(super) browse_can_edit_cell: bool,
    pub(super) browse_can_write: bool,
    pub(super) browse_can_paginate: bool,
    pub(super) browse_multiple_inspector_tabs: bool,
    pub(super) sql_confirmation: Option<SqlConfirmation>,
    pub(super) is_sql_confirmation_matched: bool,
    pub(super) is_sql_completion_visible: bool,
    pub(super) can_open_history: bool,
    pub(super) sql_tab: Option<SqlModalTab>,
    pub(super) sql_has_multiple_tabs: bool,
    pub(super) sql_compare_can_yank: bool,
    pub(super) cancel_unmatched_sql_prefix: bool,
    pub(super) can_run_diagnostics: bool,
    pub(super) is_cell_detail_searching: bool,
    pub(super) is_help_editing: bool,
    pub(super) is_confirm_blocked: bool,
    pub(super) is_service_selected: bool,
    pub(super) can_retry_connection: bool,
    pub(super) is_setup_dropdown_open: bool,
    pub(super) is_setup_dropdown_field: bool,
    pub(super) is_setup_text_focused: bool,
    pub(super) is_setup_editing: bool,
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
        self.label
    }

    pub fn from_state(state: &AppState) -> Self {
        Self::from_mode(state, state.input_mode(), true)
    }
    pub fn for_browse_state(state: &AppState) -> Self {
        Self::from_mode(state, InputMode::Normal, true)
    }
    pub(crate) fn for_help_fallback(state: &AppState) -> Self {
        Self::from_mode(state, InputMode::Normal, false)
    }
    fn from_mode(state: &AppState, mode: InputMode, use_state: bool) -> Self {
        let scope = match mode {
            InputMode::Normal if use_state => browse_scope(state),
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
            InputMode::SqlModal if matches!(state.sql_modal.status(), SqlModalStatus::Editing) => {
                Scope::SqlEditing {
                    completion_visible: state.sql_modal.completion().visible
                        && !state.sql_modal.completion().candidates.is_empty(),
                }
            }
            InputMode::SqlModal
                if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::Normal | SqlModalStatus::Success(_) | SqlModalStatus::Error(_)
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
        };
        let browse = match scope {
            Scope::Browse(browse) => browse,
            _ => BrowseScope::default(),
        };
        let history_available = !use_state || state.can_open_query_history();
        let json_searching = state.json_detail.search().is_active();
        let is_sql_editing = matches!(scope, Scope::SqlEditing { .. });
        let browse_pane = browse.pane;
        let browse_focused = browse.focused;
        let browse_read_only = browse.read_only;
        let browse_csv_available = browse.csv_available;
        let browse_connection_error = browse.connection_error;
        let browse_inspector_ddl = browse.inspector_ddl;
        let browse_staged_delete = browse.staged_delete;
        let browse_draft = browse.draft;
        let browse_yank_pending = browse.yank_pending;
        let browse_delete_pending = browse.delete_pending;
        let browse_selected_row = browse.selected_row;
        let browse_selected_cell = browse.selected_cell;
        let browse_connected = browse.connected;
        let browse_can_edit_cell = browse.can_edit_cell;
        let browse_can_write = browse.can_write;
        let browse_can_paginate = browse.can_paginate;
        let browse_multiple_inspector_tabs = browse.multiple_inspector_tabs;
        let sql_confirmation = match scope {
            Scope::SqlConfirmation { kind, .. } => Some(kind),
            _ => None,
        };
        let is_sql_confirmation_matched =
            matches!(scope, Scope::SqlConfirmation { matched: true, .. });
        let is_sql_completion_visible = matches!(
            scope,
            Scope::SqlEditing {
                completion_visible: true
            }
        );
        let can_open_history = history_available && !is_sql_completion_visible;
        let sql_tab = match scope {
            Scope::SqlNormal { tab, .. } => Some(tab),
            _ => None,
        };
        let sql_has_multiple_tabs = matches!(
            scope,
            Scope::SqlNormal {
                multiple_tabs: true,
                ..
            }
        );
        let sql_compare_can_yank = matches!(
            scope,
            Scope::SqlNormal {
                compare_can_yank: true,
                ..
            }
        );
        let cancel_unmatched_sql_prefix = matches!(
            scope,
            Scope::SqlNormal {
                tab: SqlModalTab::Plan | SqlModalTab::Compare,
                ..
            }
        );
        let can_run_diagnostics = matches!(scope, Scope::Diagnostics { can_run: true });
        let is_cell_detail_searching = matches!(scope, Scope::CellDetail { searching: true });
        let is_help_editing = matches!(scope, Scope::Help { editing: true });
        let is_confirm_blocked = matches!(scope, Scope::Confirm { blocked: true });
        let is_service_selected = matches!(
            scope,
            Scope::ConnectionSelector {
                service_selected: true
            }
        );
        let can_retry_connection = matches!(scope, Scope::ConnectionError { can_retry: true });
        let is_setup_dropdown_open = matches!(
            scope,
            Scope::ConnectionSetup {
                dropdown_open: true,
                ..
            }
        );
        let is_setup_dropdown_field = matches!(
            scope,
            Scope::ConnectionSetup {
                dropdown_field: true,
                ..
            }
        );
        let is_setup_text_focused = matches!(
            scope,
            Scope::ConnectionSetup {
                text_focused: true,
                ..
            }
        );
        let is_setup_editing = matches!(scope, Scope::ConnectionSetup { editing: true, .. });
        let label = match mode {
            InputMode::Normal => match browse.pane {
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
            InputMode::SqlModal => match scope {
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
            InputMode::JsonDetail if json_searching => "JSON Search",
            InputMode::JsonDetail => "JSON Detail",
            InputMode::JsonEdit => "JSON Edit",
            InputMode::CellDetail if is_cell_detail_searching => "Cell Detail Search",
            InputMode::CellDetail => "Cell Detail",
            InputMode::RowDetail => "Row Detail",
        };
        Self {
            preset: state.settings.saved_keymap_preset(),
            mode,
            json_searching,
            policy: FeaturePolicy::new(&state.session.active_engine_feature_profile()),
            pending_prefix: if use_state {
                state.ui.key_sequence.pending_prefix()
            } else {
                None
            },
            label,
            is_sql_editing,
            browse_pane,
            browse_focused,
            browse_read_only,
            browse_csv_available,
            browse_connection_error,
            browse_inspector_ddl,
            browse_staged_delete,
            browse_draft,
            browse_yank_pending,
            browse_delete_pending,
            browse_selected_row,
            browse_selected_cell,
            browse_connected,
            browse_can_edit_cell,
            browse_can_write,
            browse_can_paginate,
            browse_multiple_inspector_tabs,
            sql_confirmation,
            is_sql_confirmation_matched,
            is_sql_completion_visible,
            can_open_history,
            sql_tab,
            sql_has_multiple_tabs,
            sql_compare_can_yank,
            cancel_unmatched_sql_prefix,
            can_run_diagnostics,
            is_cell_detail_searching,
            is_help_editing,
            is_confirm_blocked,
            is_service_selected,
            can_retry_connection,
            is_setup_dropdown_open,
            is_setup_dropdown_field,
            is_setup_text_focused,
            is_setup_editing,
        }
    }
    pub(super) fn allows(&self, action: &Action) -> bool {
        self.policy
            .is_enabled(action.feature_requirement_for_context(self.mode, self.json_searching))
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
        inspector_ddl: state.ui.inspector_tab == InspectorTab::Ddl,
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
