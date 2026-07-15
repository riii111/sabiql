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
            save_selected_settings(state)
        }
        Action::SettingsSelectPrevious => {
            state.settings.select_previous();
            save_selected_settings(state)
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
            save_selected_settings(state)
        }
        Action::TextInput {
            target: InputTarget::SettingsErBrowser,
            ch,
        } => {
            state.settings.input_custom_browser(*ch);
            save_selected_settings(state)
        }
        Action::TextBackspace {
            target: InputTarget::SettingsErBrowser,
        } => {
            state.settings.backspace_custom_browser();
            save_selected_settings(state)
        }
        Action::TextDelete {
            target: InputTarget::SettingsErBrowser,
        } => {
            state.settings.delete_custom_browser();
            save_selected_settings(state)
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
            save_selected_settings(state)
        }
        Action::TextYank {
            target: InputTarget::SettingsErBrowser,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state
                    .settings
                    .edit_custom_browser(|input| input.yank(&killed));
            }
            save_selected_settings(state)
        }
        Action::TextMoveCursor {
            target: InputTarget::SettingsErBrowser,
            direction,
        } => {
            state.settings.move_custom_browser_cursor(*direction);
            DispatchResult::handled()
        }
        Action::SettingsApply
        | Action::SettingsCancel
        | Action::CloseModal(ModalKind::Settings) => {
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::SettingsSaved(_) => {
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

fn save_selected_settings(state: &mut AppState) -> DispatchResult {
    let settings = AppSettings {
        theme_id: state.settings.selected_theme(),
        keymap_preset: state.settings.selected_keymap_preset(),
        er_browser: state.settings.selected_er_browser(),
        wrapped_cell: state.settings.selected_wrapped_cell(),
    };
    state.ui.set_theme(settings.theme_id);
    state.ui.wrapped_cell = settings.wrapped_cell;
    state.settings.apply_selection();

    DispatchResult::handled_with(vec![Effect::SaveSettings { settings }])
}
