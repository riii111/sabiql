use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClipboardError {
    #[error("{0}")]
    Backend(Arc<arboard::Error>),
    #[error("{0}")]
    Unavailable(String),
}

impl From<arboard::Error> for ClipboardError {
    fn from(e: arboard::Error) -> Self {
        Self::Backend(Arc::new(e))
    }
}

pub trait ClipboardWriter: Send + Sync {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError>;
}
