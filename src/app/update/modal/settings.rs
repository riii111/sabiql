use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::AppSettings;

pub(in crate::update) fn open_modal_settings(state: &mut AppState) -> Vec<Effect> {
    state.settings.open(state.ui.theme_id);
    state.modal.set_mode(InputMode::Settings);
    vec![]
}

pub(in crate::update) fn settings_apply(state: &AppState) -> Vec<Effect> {
    let theme_id = state.settings.selected_theme();
    let settings = AppSettings {
        theme_id,
        icon_mode: state.settings.selected_icon_mode(),
        keymap_preset: state.settings.selected_keymap_preset(),
    };
    vec![Effect::SaveSettings { settings }]
}

pub(in crate::update) fn settings_cancel(state: &mut AppState) -> Vec<Effect> {
    state.settings.discard_selection();
    state.modal.set_mode(InputMode::Normal);
    vec![]
}

pub(in crate::update) fn settings_saved(
    state: &mut AppState,
    outcome: Result<AppSettings, String>,
    now: Instant,
) -> Vec<Effect> {
    let settings = match outcome {
        Ok(settings) => settings,
        Err(error) => {
            state
                .messages
                .set_error(format!("Failed to save settings: {error}"));
            return vec![];
        }
    };
    state.ui.theme_id = settings.theme_id;
    state.settings.commit_saved(
        settings.theme_id,
        settings.icon_mode,
        settings.keymap_preset,
    );
    state
        .messages
        .set_success_at("Settings saved".to_string(), now);
    vec![]
}
