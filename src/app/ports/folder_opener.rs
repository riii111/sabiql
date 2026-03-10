use std::path::Path;

pub trait FolderOpener: Send + Sync {
    fn open(&self, path: &Path) -> Result<(), String>;
}
