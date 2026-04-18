use std::sync::Arc;

use crate::domain::ParseCommandTagError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DbOperationError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[source] Arc<serde_json::Error>),
    #[error("Empty response: {0}")]
    EmptyResponse(String),
    #[error("CSV parse error: {0}")]
    CsvParse(#[source] Arc<csv::Error>),
    #[error("Command tag parse failed: {0}")]
    CommandTagParseFailed(#[from] ParseCommandTagError),
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("Operation timed out: {0}")]
    Timeout(String),
}

impl From<serde_json::Error> for DbOperationError {
    fn from(e: serde_json::Error) -> Self {
        Self::InvalidJson(Arc::new(e))
    }
}

impl From<csv::Error> for DbOperationError {
    fn from(e: csv::Error) -> Self {
        Self::CsvParse(Arc::new(e))
    }
}
