use std::path::Path;

use crate::app::ports::FolderOpener;

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
}
