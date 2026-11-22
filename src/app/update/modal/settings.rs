use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::picker::sanitize_filter_text;
use crate::ports::outbound::AppSettings;

pub(in crate::update) fn paste_custom_browser(state: &mut AppState, text: String) -> Vec<Effect> {
    state
        .settings
        .edit_custom_browser(|input| input.insert_str(&sanitize_filter_text(&text)));
    vec![]
}

pub(in crate::update) fn open_modal_settings(state: &mut AppState) -> Vec<Effect> {
    state.settings.open(state.ui.theme_id());
    state.modal.set_mode(InputMode::Settings);
    vec![]
}

pub(in crate::update) fn settings_select_next(state: &mut AppState) -> Vec<Effect> {
    state.settings.select_next();
    vec![]
}

pub(in crate::update) fn settings_select_previous(state: &mut AppState) -> Vec<Effect> {
    state.settings.select_previous();
    vec![]
}

pub(in crate::update) fn settings_next_section(state: &mut AppState) -> Vec<Effect> {
    state.settings.switch_next_section();
    vec![]
}

pub(in crate::update) fn settings_previous_section(state: &mut AppState) -> Vec<Effect> {
    state.settings.switch_previous_section();
    vec![]
}

pub(in crate::update) fn settings_start_custom_browser_edit(state: &mut AppState) -> Vec<Effect> {
    state.settings.start_custom_browser_edit();
    vec![]
}

pub(in crate::update) fn settings_stop_custom_browser_edit(state: &mut AppState) -> Vec<Effect> {
    state.settings.stop_custom_browser_edit();
    vec![]
}

pub(in crate::update) fn settings_apply(state: &AppState) -> Vec<Effect> {
    let theme_id = state.settings.selected_theme();
    let settings = AppSettings {
        theme_id,
        icon_mode: state.settings.selected_icon_mode(),
        keymap_preset: state.settings.selected_keymap_preset(),
        er_browser: state.settings.selected_er_browser(),
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
    settings: AppSettings,
    now: Instant,
) -> Vec<Effect> {
    state.ui.set_theme(settings.theme_id);
    state.settings.commit_saved(
        settings.theme_id,
        settings.icon_mode,
        settings.keymap_preset,
        settings.er_browser,
    );
    state
        .messages
        .set_success_at("Settings saved".to_string(), now);
    vec![]
}

pub(in crate::update) fn settings_save_failed(state: &mut AppState, error: String) -> Vec<Effect> {
    state
        .messages
        .set_error(format!("Failed to save settings: {error}"));
    vec![]
}
