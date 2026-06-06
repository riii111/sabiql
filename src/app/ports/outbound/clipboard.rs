use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClipboardError {
    #[error("{0}")]
    Backend(#[source] Arc<dyn std::error::Error + Send + Sync>),
    #[error("{0}")]
    Unavailable(String),
}

impl ClipboardError {
    pub fn backend(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Backend(Arc::new(error))
    }
}

pub trait ClipboardWriter: Send + Sync {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError>;
}
