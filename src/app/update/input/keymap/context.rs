use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::FeaturePolicy;
use crate::update::action::Action;

#[derive(Clone)]
pub struct KeyContext {
    pub(super) preset: KeymapPreset,
    mode: InputMode,
    json_mode: JsonDetailMode,
    policy: FeaturePolicy,
}

impl KeyContext {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            preset: state.settings.saved_keymap_preset(),
            mode: state.input_mode(),
            json_mode: state.json_detail.mode(),
            policy: FeaturePolicy::new(&state.session.active_engine_feature_profile()),
        }
    }

    pub(crate) fn for_scope(mode: InputMode, preset: KeymapPreset, policy: &FeaturePolicy) -> Self {
        Self {
            preset,
            mode,
            json_mode: JsonDetailMode::Viewing,
            policy: policy.clone(),
        }
    }

    pub(super) fn allows(&self, action: &Action) -> bool {
        self.policy
            .is_enabled(action.feature_requirement_for_context(self.mode, self.json_mode))
    }
}
