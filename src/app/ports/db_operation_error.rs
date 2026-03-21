#[derive(Debug, Clone, thiserror::Error)]
pub enum DbOperationError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("Operation timed out")]
    Timeout,
}
