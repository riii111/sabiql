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
