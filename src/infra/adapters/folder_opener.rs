use std::path::Path;

use crate::app::ports::folder_opener::{FolderOpenError, FolderOpener};

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) -> Result<(), FolderOpenError> {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(path).spawn();
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        let result = std::process::Command::new("xdg-open").arg(path).spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("explorer").arg(path).spawn();
        #[cfg(not(any(
            target_os = "freebsd",
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )))]
        compile_error!("FolderOpener: unsupported target OS");

        result.map(|_| ()).map_err(|e| FolderOpenError {
            message: e.to_string(),
        })
    }
}
