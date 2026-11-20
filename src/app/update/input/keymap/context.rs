use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::connection::list::is_service_selected;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::settings::{ErBrowserChoice, KeymapPreset, SettingsSection};
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::Action;

#[derive(Clone)]
pub struct KeyContext {
    pub(super) preset: KeymapPreset,
    mode: InputMode,
    json_mode: JsonDetailMode,
    policy: FeaturePolicy,
    scope: Scope,
}

#[derive(Clone, Copy)]
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
    Settings {
        editing: bool,
        custom_selected: bool,
    },
    Other,
}

impl KeyContext {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            preset: state.settings.saved_keymap_preset(),
            mode: state.input_mode(),
            json_mode: state.json_detail.mode(),
            policy: FeaturePolicy::new(&state.session.active_engine_feature_profile()),
            scope: match state.input_mode() {
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
                InputMode::Settings => Scope::Settings {
                    editing: state.settings.is_editing_custom_er_browser(),
                    custom_selected: state.settings.section() == SettingsSection::ErDiagram
                        && state.settings.selected_er_browser_choice() == ErBrowserChoice::Custom,
                },
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
            scope: match mode {
                InputMode::SqliteDiagnostics => Scope::Diagnostics { can_run: true },
                InputMode::CellDetail => Scope::CellDetail { searching: false },
                InputMode::Help => Scope::Help { editing: false },
                InputMode::ConfirmDialog => Scope::Confirm { blocked: false },
                InputMode::ConnectionSelector => Scope::ConnectionSelector {
                    service_selected: false,
                },
                InputMode::ConnectionError => Scope::ConnectionError { can_retry: false },
                InputMode::Settings => Scope::Settings {
                    editing: false,
                    custom_selected: true,
                },
                _ => Scope::Other,
            },
        }
    }

    pub(super) fn allows(&self, action: &Action) -> bool {
        self.policy
            .is_enabled(action.feature_requirement_for_context(self.mode, self.json_mode))
    }

    pub(super) fn supports_diagnostics(&self) -> bool {
        self.policy
            .is_enabled(FeatureRequirement::SqliteDiagnostics)
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

    pub(super) fn is_settings_editing(&self) -> bool {
        matches!(self.scope, Scope::Settings { editing: true, .. })
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

    pub(crate) fn with_help_editing(mut self, editing: bool) -> Self {
        self.mode = InputMode::Help;
        self.scope = Scope::Help { editing };
        self
    }
}
