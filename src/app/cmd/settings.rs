use crate::ports::outbound::{AppSettings, SettingsStore};
use crate::update::action::Action;

pub(in crate::cmd) fn spawn(
    settings: AppSettings,
    settings_store: &std::sync::Arc<dyn SettingsStore>,
    action_tx: &tokio::sync::mpsc::Sender<Action>,
) {
    let store = std::sync::Arc::clone(settings_store);
    let tx = action_tx.clone();
    std::thread::spawn(move || {
        let _ = tx.blocking_send(run(settings, &store));
    });
}

pub(in crate::cmd) fn run(
    settings: AppSettings,
    settings_store: &std::sync::Arc<dyn SettingsStore>,
) -> Action {
    let result = settings_store.save(settings.clone());
    match result {
        Ok(()) => Action::SettingsSaved(settings),
        Err(error) => Action::SettingsSaveFailed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::model::shared::settings::ClipboardBackend;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::model::shared::settings::KeymapPreset;
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

    #[tokio::test]
    async fn pending_store_save_does_not_block_event_processing() {
        struct DelayedStore(std::sync::Mutex<std::sync::mpsc::Receiver<()>>);
        impl SettingsStore for DelayedStore {
            fn save(&self, _: AppSettings) -> Result<(), SettingsStoreError> {
                self.0.lock().unwrap().recv().unwrap();
                Ok(())
            }
        }
        let (release, wait) = std::sync::mpsc::channel();
        let store: Arc<dyn SettingsStore> = Arc::new(DelayedStore(Mutex::new(wait)));
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        spawn(AppSettings::default(), &store, &tx);
        assert!(rx.try_recv().is_err());
        release.send(()).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap();
        assert!(matches!(result, Some(Action::SettingsSaved(_))));
    }

    #[test]
    fn save_settings_dispatches_saved_action() {
        let store = Arc::new(RecordingSettingsStore {
            saved: Mutex::new(Vec::new()),
        });

        let action = run(
            AppSettings {
                clipboard_backend: ClipboardBackend::Auto,
                theme_id: ThemeId::Light,
                keymap_preset: KeymapPreset::Ide,
                er_browser: Some("Firefox".to_string()),
            },
            &(store.clone() as Arc<dyn SettingsStore>),
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
            Action::SettingsSaved(settings)
                if settings.theme_id == ThemeId::Light
                    && settings.keymap_preset == KeymapPreset::Ide
                    && settings.er_browser.as_deref() == Some("Firefox")
        ));
    }

    #[test]
    fn save_settings_dispatches_save_failed_action() {
        let store = Arc::new(FailingSettingsStore);

        let action = run(
            AppSettings {
                clipboard_backend: ClipboardBackend::Auto,
                theme_id: ThemeId::Light,
                keymap_preset: KeymapPreset::default(),
                er_browser: None,
            },
            &(store as Arc<dyn SettingsStore>),
        );

        assert!(matches!(
            action,
            Action::SettingsSaveFailed(error) if error == "I/O error: disk full"
        ));
    }
}
