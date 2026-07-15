use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::TextInputEditing;
use crate::ports::outbound::AppSettings;
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_settings(
    state: &mut AppState,
    action: &Action,
    now: Instant,
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
        Action::SettingsToggleWrappedCellScroll => {
            state.settings.toggle_wrapped_cell_horizontal();
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
        Action::TextKill {
            target: InputTarget::SettingsErBrowser,
            direction,
        } => {
            if let Some(killed) = state
                .settings
                .edit_custom_browser(|input| input.kill(*direction))
            {
                state.record_kill(killed);
            }
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::SettingsErBrowser,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state
                    .settings
                    .edit_custom_browser(|input| input.yank(&killed));
            }
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
                keymap_preset: state.settings.selected_keymap_preset(),
                er_browser: state.settings.selected_er_browser(),
                wrapped_cell: state.settings.selected_wrapped_cell(),
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
            state.settings.commit_saved(
                settings.theme_id,
                settings.keymap_preset,
                settings.er_browser.clone(),
                settings.wrapped_cell,
            );
            state.ui.wrapped_cell = settings.wrapped_cell;
            state
                .messages
                .set_success_at("Settings saved".to_string(), now);
            DispatchResult::handled()
        }
        Action::SettingsSaveFailed(error) => {
            state
                .messages
                .set_error_at(format!("Failed to save settings: {error}"), now);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
