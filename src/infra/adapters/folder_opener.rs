use std::path::Path;

use crate::app::ports::FolderOpener;

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) {
        #[cfg(target_os = "macos")]
        std::process::Command::new("open").arg(path).spawn().ok();
        #[cfg(target_os = "linux")]
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .ok();
        #[cfg(target_os = "windows")]
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .ok();
    }
}
