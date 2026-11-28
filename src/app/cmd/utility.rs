use std::path::Path;
use std::sync::Arc;

use crate::ports::outbound::{ClipboardWriter, FolderOpener};
use crate::update::action::{Action, CopyTarget};

pub(in crate::cmd) async fn copy_to_clipboard(
    clipboard: Arc<dyn ClipboardWriter>,
    content: String,
    target: CopyTarget,
) -> Action {
    let outcome = tokio::task::spawn_blocking(move || clipboard.copy_text(&content))
        .await
        .expect("clipboard task panicked");
    Action::Copied { target, outcome }
}

pub(in crate::cmd) fn open_folder(folder_opener: &dyn FolderOpener, path: &Path) -> Option<Action> {
    folder_opener
        .open(path)
        .err()
        .map(|error| Action::OpenFolderFailed(Arc::new(error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use crate::ports::outbound::clipboard::ClipboardError;

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
        result: Result<(), String>,
    }

    impl FolderOpener for MockFolderOpener {
        fn open(&self, path: &Path) -> Result<(), std::io::Error> {
            self.opened.lock().unwrap().push(path.to_path_buf());
            match &self.result {
                Ok(()) => Ok(()),
                Err(error) => Err(std::io::Error::other(error.as_str())),
            }
        }
    }

    #[tokio::test]
    async fn copy_reports_clipboard_outcome_with_target() {
        let failing = Arc::new(MockClipboard {
            result: Err(ClipboardError::Unavailable("clipboard error".to_string())),
        });

        let copied = copy_to_clipboard(
            Arc::new(MockClipboard { result: Ok(()) }),
            "hello".to_string(),
            CopyTarget::ConnectionError,
        )
        .await;
        let failed = copy_to_clipboard(
            failing,
            "hello".to_string(),
            CopyTarget::ResultRow { row: 1 },
        )
        .await;

        assert!(matches!(
            copied,
            Action::Copied {
                target: CopyTarget::ConnectionError,
                outcome: Ok(())
            }
        ));
        match failed {
            Action::Copied {
                target: CopyTarget::ResultRow { row: 1 },
                outcome: Err(error),
            } => assert_eq!(error.to_string(), "clipboard error"),
            other => panic!("expected failed copy, got {other:?}"),
        }
    }

    #[test]
    fn open_folder_reports_only_failures() {
        let opener = MockFolderOpener {
            opened: Mutex::new(vec![]),
            result: Ok(()),
        };
        let failing = MockFolderOpener {
            opened: Mutex::new(vec![]),
            result: Err("No such file or directory".to_owned()),
        };

        assert!(open_folder(&opener, Path::new("/tmp/export")).is_none());
        let failure = open_folder(&failing, Path::new("/nonexistent"));

        assert_eq!(
            *opener.opened.lock().unwrap(),
            [PathBuf::from("/tmp/export")]
        );
        match failure {
            Some(Action::OpenFolderFailed(e)) => {
                assert_eq!(e.to_string(), "No such file or directory");
            }
            other => panic!("expected OpenFolderFailed, got {other:?}"),
        }
    }
}
