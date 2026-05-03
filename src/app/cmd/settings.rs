use tokio::sync::mpsc;

use crate::cmd::effect::Effect;
use crate::ports::outbound::{AppSettings, SettingsStore};
use crate::update::action::Action;

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    settings_store: &std::sync::Arc<dyn SettingsStore>,
) {
    let Effect::SaveSettings { theme_id } = effect else {
        return;
    };

    let result = settings_store.save(AppSettings { theme_id });
    let action = match result {
        Ok(()) => Action::SettingsSaved,
        Err(error) => Action::SettingsSaveFailed(error),
    };
    let _ = action_tx.send(action).await;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::sync::mpsc;

    use super::*;
    use crate::model::shared::theme_id::ThemeId;
    use crate::ports::outbound::{AppSettings, SettingsStoreError};

    struct RecordingSettingsStore {
        saved: Mutex<Vec<AppSettings>>,
    }

    struct FailingSettingsStore;

    impl SettingsStore for RecordingSettingsStore {
        fn load(&self) -> Result<AppSettings, SettingsStoreError> {
            Ok(AppSettings::default())
        }

        fn save(&self, settings: AppSettings) -> Result<(), SettingsStoreError> {
            self.saved.lock().unwrap().push(settings);
            Ok(())
        }
    }

    impl SettingsStore for FailingSettingsStore {
        fn load(&self) -> Result<AppSettings, SettingsStoreError> {
            Ok(AppSettings::default())
        }

        fn save(&self, _settings: AppSettings) -> Result<(), SettingsStoreError> {
            Err(SettingsStoreError::Io(Arc::new(std::io::Error::other(
                "disk full",
            ))))
        }
    }

    #[tokio::test]
    async fn save_settings_dispatches_saved_action() {
        let store = Arc::new(RecordingSettingsStore {
            saved: Mutex::new(Vec::new()),
        });
        let (tx, mut rx) = mpsc::channel(1);

        run(
            Effect::SaveSettings {
                theme_id: ThemeId::Light,
            },
            &tx,
            &(store.clone() as Arc<dyn SettingsStore>),
        )
        .await;

        assert_eq!(store.saved.lock().unwrap()[0].theme_id, ThemeId::Light);
        assert!(matches!(rx.recv().await, Some(Action::SettingsSaved)));
    }

    #[tokio::test]
    async fn save_settings_dispatches_save_failed_action() {
        let store = Arc::new(FailingSettingsStore);
        let (tx, mut rx) = mpsc::channel(1);

        run(
            Effect::SaveSettings {
                theme_id: ThemeId::Light,
            },
            &tx,
            &(store as Arc<dyn SettingsStore>),
        )
        .await;

        assert!(matches!(
            rx.recv().await,
            Some(Action::SettingsSaveFailed(_))
        ));
    }
}
