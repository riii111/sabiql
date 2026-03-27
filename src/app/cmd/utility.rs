use std::sync::Arc;

use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::app::cmd::effect::Effect;
use crate::app::ports::{ClipboardWriter, FolderOpener};
use crate::app::update::action::Action;

pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    clipboard: &Arc<dyn ClipboardWriter>,
    folder_opener: &Arc<dyn FolderOpener>,
) -> Result<()> {
    match effect {
        Effect::CopyToClipboard {
            content,
            on_success,
            on_failure,
        } => {
            let clipboard = Arc::clone(clipboard);
            let tx = action_tx.clone();
            tokio::task::spawn_blocking(move || match clipboard.copy_text(&content) {
                Ok(()) => {
                    if let Some(action) = on_success {
                        tx.blocking_send(action).ok();
                    }
                }
                Err(e) => {
                    if let Some(action) = on_failure {
                        tx.blocking_send(action).ok();
                    } else {
                        tx.blocking_send(Action::CopyFailed(e)).ok();
                    }
                }
            });
        }
        Effect::OpenFolder { path } => {
            if let Err(e) = folder_opener.open(&path) {
                action_tx.send(Action::OpenFolderFailed(e)).await.ok();
            }
        }
        _ => unreachable!("utility::run called with non-utility effect"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use crate::app::ports::clipboard::ClipboardError;
    use crate::app::ports::folder_opener::FolderOpenError;

    struct MockClipboard {
        result: Result<(), ClipboardError>,
    }

    impl ClipboardWriter for MockClipboard {
        fn copy_text(&self, _content: &str) -> Result<(), ClipboardError> {
            self.result.clone()
        }
    }

    struct MockFolderOpener {
        opened: Mutex<Vec<PathBuf>>,
        result: Result<(), FolderOpenError>,
    }

    impl MockFolderOpener {
        fn new() -> Self {
            Self {
                opened: Mutex::new(vec![]),
                result: Ok(()),
            }
        }

        fn failing(error: &str) -> Self {
            Self {
                opened: Mutex::new(vec![]),
                result: Err(FolderOpenError {
                    message: error.to_string(),
                }),
            }
        }
    }

    impl FolderOpener for MockFolderOpener {
        fn open(&self, path: &Path) -> Result<(), FolderOpenError> {
            self.opened.lock().unwrap().push(path.to_path_buf());
            self.result.clone()
        }
    }

    mod copy_to_clipboard {
        use super::*;

        #[tokio::test]
        async fn on_success_dispatched_when_copy_succeeds() {
            let (tx, mut rx) = mpsc::channel(8);
            let clipboard: Arc<dyn ClipboardWriter> = Arc::new(MockClipboard { result: Ok(()) });
            let folder_opener: Arc<dyn FolderOpener> = Arc::new(MockFolderOpener::new());

            run(
                Effect::CopyToClipboard {
                    content: "hello".to_string(),
                    on_success: Some(Action::Render),
                    on_failure: None,
                },
                &tx,
                &clipboard,
                &folder_opener,
            )
            .await
            .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(matches!(action, Action::Render));
        }

        #[tokio::test]
        async fn on_failure_dispatched_when_copy_fails() {
            let (tx, mut rx) = mpsc::channel(8);
            let clipboard: Arc<dyn ClipboardWriter> = Arc::new(MockClipboard {
                result: Err(ClipboardError {
                    message: "clipboard error".to_string(),
                }),
            });
            let folder_opener: Arc<dyn FolderOpener> = Arc::new(MockFolderOpener::new());

            run(
                Effect::CopyToClipboard {
                    content: "hello".to_string(),
                    on_success: None,
                    on_failure: Some(Action::Render),
                },
                &tx,
                &clipboard,
                &folder_opener,
            )
            .await
            .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            assert!(matches!(action, Action::Render));
        }

        #[tokio::test]
        async fn copy_failed_dispatched_when_no_on_failure() {
            let (tx, mut rx) = mpsc::channel(8);
            let clipboard: Arc<dyn ClipboardWriter> = Arc::new(MockClipboard {
                result: Err(ClipboardError {
                    message: "clipboard error".to_string(),
                }),
            });
            let folder_opener: Arc<dyn FolderOpener> = Arc::new(MockFolderOpener::new());

            run(
                Effect::CopyToClipboard {
                    content: "hello".to_string(),
                    on_success: None,
                    on_failure: None,
                },
                &tx,
                &clipboard,
                &folder_opener,
            )
            .await
            .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            match action {
                Action::CopyFailed(e) => assert_eq!(e.message, "clipboard error"),
                other => panic!("expected CopyFailed, got {other:?}"),
            }
        }
    }

    mod open_folder {
        use super::*;

        #[tokio::test]
        async fn calls_folder_opener_port() {
            let (tx, _rx) = mpsc::channel(8);
            let clipboard: Arc<dyn ClipboardWriter> = Arc::new(MockClipboard { result: Ok(()) });
            let opener = Arc::new(MockFolderOpener::new());
            let folder_opener: Arc<dyn FolderOpener> = Arc::clone(&opener) as _;

            run(
                Effect::OpenFolder {
                    path: PathBuf::from("/tmp/export"),
                },
                &tx,
                &clipboard,
                &folder_opener,
            )
            .await
            .unwrap();

            let opened = opener.opened.lock().unwrap();
            assert_eq!(opened.len(), 1);
            assert_eq!(opened[0], PathBuf::from("/tmp/export"));
        }

        #[tokio::test]
        async fn failure_dispatches_open_folder_failed() {
            let (tx, mut rx) = mpsc::channel(8);
            let clipboard: Arc<dyn ClipboardWriter> = Arc::new(MockClipboard { result: Ok(()) });
            let opener = Arc::new(MockFolderOpener::failing("No such file or directory"));
            let folder_opener: Arc<dyn FolderOpener> = Arc::clone(&opener) as _;

            run(
                Effect::OpenFolder {
                    path: PathBuf::from("/nonexistent"),
                },
                &tx,
                &clipboard,
                &folder_opener,
            )
            .await
            .unwrap();

            let action = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                .await
                .expect("action timeout")
                .expect("channel closed");
            match action {
                Action::OpenFolderFailed(e) => {
                    assert_eq!(e.message, "No such file or directory");
                }
                other => panic!("expected OpenFolderFailed, got {other:?}"),
            }
        }
    }
}
