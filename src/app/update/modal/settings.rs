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
                clipboard_backend: state.settings.selected_clipboard_backend(),
                theme_id,
                keymap_preset: state.settings.selected_keymap_preset(),
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
            state.settings.commit_saved(
                settings.theme_id,
                settings.keymap_preset,
                settings.er_browser.clone(),
                settings.clipboard_backend,
            );
            state
                .messages
                .set_success_at("Settings saved".to_string(), now);
            DispatchResult::handled()
        }
        Action::SettingsSaveFailed(error) => {
            state
                .messages
                .set_error(format!("Failed to save settings: {error}"));
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::settings::ClipboardBackend;

    #[test]
    fn clipboard_selection_only_applies_after_save_success() {
        let mut state = AppState::new("test".into());
        let now = Instant::now();
        reduce_settings(&mut state, &Action::OpenModal(ModalKind::Settings), now);
        reduce_settings(&mut state, &Action::SettingsPreviousSection, now);
        reduce_settings(&mut state, &Action::SettingsSelectNext, now);
        let effects = reduce_settings(&mut state, &Action::SettingsApply, now)
            .into_effects()
            .unwrap();
        let Effect::SaveSettings { settings } = &effects[0] else {
            panic!("expected save")
        };
        assert_eq!(settings.clipboard_backend, ClipboardBackend::Native);
        assert_eq!(
            state.settings.saved_clipboard_backend(),
            ClipboardBackend::Auto
        );

        reduce_settings(
            &mut state,
            &Action::SettingsSaveFailed("disk full".into()),
            now,
        );
        assert_eq!(
            state.settings.saved_clipboard_backend(),
            ClipboardBackend::Auto
        );
        reduce_settings(&mut state, &Action::SettingsCancel, now);
        assert_eq!(
            state.settings.selected_clipboard_backend(),
            ClipboardBackend::Auto
        );

        reduce_settings(&mut state, &Action::SettingsSaved(settings.clone()), now);
        assert_eq!(
            state.settings.saved_clipboard_backend(),
            ClipboardBackend::Native
        );
    }
}
