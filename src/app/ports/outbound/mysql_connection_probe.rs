use async_trait::async_trait;

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MySqlConnectionProbe: Send + Sync {
    async fn probe(&self, dsn: &str) -> Result<(), DbOperationError>;
}
