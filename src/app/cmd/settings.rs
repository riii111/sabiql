use crate::ports::outbound::{AppSettings, SettingsStore};
use crate::update::action::Action;

pub(in crate::cmd) fn save(settings: AppSettings, settings_store: &dyn SettingsStore) -> Action {
    let outcome = settings_store
        .save(settings.clone())
        .map(|()| settings)
        .map_err(|error| error.to_string());
    Action::SettingsSaved(outcome)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::model::shared::settings::{IconMode, KeymapPreset};
    use crate::model::shared::theme_id::ThemeId;
    use crate::ports::outbound::SettingsStoreError;

    struct RecordingSettingsStore {
        saved: Mutex<Vec<AppSettings>>,
    }

    struct FailingSettingsStore;

    impl SettingsStore for RecordingSettingsStore {
        fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
            self.saved.lock().unwrap().push(settings);
            Ok(())
        }
    }

    impl SettingsStore for FailingSettingsStore {
        fn save(&self, _settings: AppSettings) -> Result<(), SettingsStoreError> {
            Err(SettingsStoreError::Io(Arc::new(std::io::Error::other(
                "disk full",
            ))))
        }
    }

    #[test]
    fn save_settings_dispatches_saved_action() {
        let store = Arc::new(RecordingSettingsStore {
            saved: Mutex::new(Vec::new()),
        });

        let action = save(
            AppSettings {
                theme_id: ThemeId::Light,
                icon_mode: IconMode::default(),
                keymap_preset: KeymapPreset::Ide,
                er_browser: Some("Firefox".to_string()),
            },
            store.as_ref(),
        );

        assert_eq!(store.saved.lock().unwrap()[0].theme_id, ThemeId::Light);
        assert_eq!(
            store.saved.lock().unwrap()[0].er_browser.as_deref(),
            Some("Firefox")
        );
        assert_eq!(
            store.saved.lock().unwrap()[0].keymap_preset,
            KeymapPreset::Ide
        );
        assert!(matches!(
            action,
            Action::SettingsSaved(Ok(settings))
                if settings.theme_id == ThemeId::Light
                    && settings.keymap_preset == KeymapPreset::Ide
                    && settings.er_browser.as_deref() == Some("Firefox")
        ));
    }

    #[test]
    fn save_settings_dispatches_save_failed_action() {
        let store = FailingSettingsStore;

        let action = save(
            AppSettings {
                theme_id: ThemeId::Light,
                icon_mode: IconMode::default(),
                keymap_preset: KeymapPreset::default(),
                er_browser: None,
            },
            &store,
        );

        assert!(matches!(
            action,
            Action::SettingsSaved(Err(error)) if error == "I/O error: disk full"
        ));
    }
}
