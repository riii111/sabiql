use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClipboardError {
    #[error("{0}")]
    Backend(#[source] Arc<dyn std::error::Error + Send + Sync>),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Terminal(String),
}

impl ClipboardError {
    pub fn backend(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Backend(Arc::new(error))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOutcome {
    Copied,
    SentToTerminal,
}

pub trait ClipboardWriter: Send + Sync {
    fn copy_text(&self, content: &str) -> Result<ClipboardOutcome, ClipboardError>;
}
