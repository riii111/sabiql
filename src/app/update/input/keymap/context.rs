use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::Action;

#[derive(Clone)]
pub struct KeyContext {
    pub(super) preset: KeymapPreset,
    mode: InputMode,
    json_mode: JsonDetailMode,
    policy: FeaturePolicy,
    pub(super) can_run_diagnostics: bool,
    pub(super) cell_detail_searching: bool,
    pub(super) help_editing: bool,
}

impl KeyContext {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            preset: state.settings.saved_keymap_preset(),
            mode: state.input_mode(),
            json_mode: state.json_detail.mode(),
            policy: FeaturePolicy::new(&state.session.active_engine_feature_profile()),
            can_run_diagnostics: state.sqlite_diagnostics.can_run_quick_check(),
            cell_detail_searching: state.cell_detail.search().is_active(),
            help_editing: state.ui.help().mode() == HelpMode::EditingFilter,
        }
    }

    pub(crate) fn for_scope(mode: InputMode, preset: KeymapPreset, policy: &FeaturePolicy) -> Self {
        Self {
            preset,
            mode,
            json_mode: JsonDetailMode::Viewing,
            policy: policy.clone(),
            can_run_diagnostics: true,
            cell_detail_searching: false,
            help_editing: false,
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

    pub(crate) fn with_cell_detail_searching(mut self, searching: bool) -> Self {
        self.cell_detail_searching = searching;
        self
    }

    pub(crate) fn with_help_editing(mut self, editing: bool) -> Self {
        self.help_editing = editing;
        self
    }
}
