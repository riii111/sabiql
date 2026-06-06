use std::path::Path;

use crate::app::ports::outbound::folder_opener::{FolderOpenError, FolderOpener};

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) -> Result<(), FolderOpenError> {
        open_folder(path)
    }
}

#[cfg(target_os = "macos")]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    std::process::Command::new("open").arg(path).spawn()?;
    Ok(())
}

#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    std::process::Command::new("explorer").arg(path).spawn()?;
    Ok(())
}

#[cfg(not(any(
    target_os = "freebsd",
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
)))]
fn open_folder(_path: &Path) -> Result<(), FolderOpenError> {
    Err(std::io::Error::other("Opening folders is unsupported on this platform").into())
}
