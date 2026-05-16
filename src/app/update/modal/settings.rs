use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::AppSettings;
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_settings(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::Settings) => {
            state.settings.open(state.ui.theme_id());
            state.modal.set_mode(InputMode::Settings);
            DispatchResult::handled()
        }
        Action::SettingsSelectNext => {
            state.settings.select_next();
            DispatchResult::handled()
        }
        Action::SettingsSelectPrevious => {
            state.settings.select_previous();
            DispatchResult::handled()
        }
        Action::SettingsNextSection => {
            state.settings.switch_next_section();
            DispatchResult::handled()
        }
        Action::SettingsPreviousSection => {
            state.settings.switch_previous_section();
            DispatchResult::handled()
        }
        Action::SettingsStartCustomBrowserEdit => {
            state.settings.start_custom_browser_edit();
            DispatchResult::handled()
        }
        Action::SettingsStopCustomBrowserEdit => {
            state.settings.stop_custom_browser_edit();
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::SettingsErBrowser,
            ch,
        } => {
            state.settings.input_custom_browser(*ch);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::SettingsErBrowser,
        } => {
            state.settings.backspace_custom_browser();
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::SettingsErBrowser,
        } => {
            state.settings.delete_custom_browser();
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction,
        } => {
            state.settings.move_custom_browser_cursor(*direction);
            DispatchResult::handled()
        }
        Action::SettingsApply => {
            let theme_id = state.settings.selected_theme();
            let settings = AppSettings {
                theme_id,
                er_browser: state.settings.selected_er_browser(),
            };
            DispatchResult::handled_with(vec![Effect::SaveSettings { settings }])
        }
        Action::SettingsCancel | Action::CloseModal(ModalKind::Settings) => {
            state.settings.discard_selection();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::SettingsSaved(settings) => {
            state.ui.set_theme(settings.theme_id);
            state
                .settings
                .commit_saved(settings.theme_id, settings.er_browser.clone());
            state.set_success("Settings saved".to_string());
            DispatchResult::handled()
        }
        Action::SettingsSaveFailed(error) => {
            state.set_error(format!("Failed to save settings: {error}"));
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
