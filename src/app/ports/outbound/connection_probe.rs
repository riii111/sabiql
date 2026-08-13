use async_trait::async_trait;

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ConnectionProbe: Send + Sync {
    async fn probe(&self, dsn: &str) -> Result<(), DbOperationError>;

    async fn fetch_databases(&self, _dsn: &str) -> Result<Vec<String>, DbOperationError> {
        Err(DbOperationError::UnsupportedOperation(
            "Database listing is only implemented for MySQL".to_string(),
        ))
    }
}
