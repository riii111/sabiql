#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ClipboardError {
    pub message: String,
}

pub trait ClipboardWriter: Send + Sync {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError>;
}
