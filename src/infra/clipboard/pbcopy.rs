use std::io::Write;
use std::process::{Command, Stdio};

use crate::app::ports::{ClipboardError, ClipboardWriter};

/// macOS clipboard adapter using pbcopy command.
pub struct PbcopyAdapter;

impl PbcopyAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PbcopyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardWriter for PbcopyAdapter {
    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| ClipboardError::CommandNotFound(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(content.as_bytes())
                .map_err(|e| ClipboardError::WriteFailed(e.to_string()))?;
        }

        let status = child
            .wait()
            .map_err(|e| ClipboardError::WriteFailed(e.to_string()))?;

        if !status.success() {
            return Err(ClipboardError::WriteFailed(format!(
                "pbcopy exited with status: {}",
                status
            )));
        }

        Ok(())
    }
}
