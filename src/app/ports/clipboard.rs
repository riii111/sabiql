use std::error::Error;
use std::fmt;

#[derive(Debug, Clone)]
pub enum ClipboardError {
    CommandNotFound(String),
    WriteFailed(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::CommandNotFound(msg) => write!(f, "Command not found: {}", msg),
            ClipboardError::WriteFailed(msg) => write!(f, "Clipboard write failed: {}", msg),
        }
    }
}

impl Error for ClipboardError {}

/// Trait for writing to the system clipboard.
pub trait ClipboardWriter: Send + Sync {
    /// Write the given content to the system clipboard.
    fn write(&self, content: &str) -> Result<(), ClipboardError>;
}
