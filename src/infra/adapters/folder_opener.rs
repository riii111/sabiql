use std::path::Path;
use std::process::Command;

use crate::app::ports::outbound::folder_opener::{FolderOpenError, FolderOpener};

pub struct NativeFolderOpener;

impl FolderOpener for NativeFolderOpener {
    fn open(&self, path: &Path) -> Result<(), FolderOpenError> {
        open_folder(path)
    }
}

#[cfg(target_os = "macos")]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    spawn_folder_opener("open", &[], path)
}

#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    spawn_folder_opener("xdg-open", &[], path)
}

#[cfg(target_os = "windows")]
fn open_folder(path: &Path) -> Result<(), FolderOpenError> {
    spawn_folder_opener("explorer", &[], path)
}

#[cfg(any(
    target_os = "freebsd",
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
))]
fn spawn_folder_opener(program: &str, args: &[&str], path: &Path) -> Result<(), FolderOpenError> {
    Command::new(program).args(args).arg(path).spawn()?;
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
