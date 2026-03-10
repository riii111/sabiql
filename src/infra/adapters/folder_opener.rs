use std::path::Path;

use crate::app::ports::FolderOpener;

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(path).spawn();
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open").arg(path).spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("explorer").arg(path).spawn();

        result.map(|_| ()).map_err(|e| e.to_string())
    }
}
