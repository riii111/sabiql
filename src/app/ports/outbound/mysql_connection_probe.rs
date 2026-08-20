use async_trait::async_trait;

use super::DbOperationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MySqlConnectionProbeResult {
    pub lower_case_table_names: u8,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MySqlConnectionProbe: Send + Sync {
    async fn probe(&self, dsn: &str) -> Result<MySqlConnectionProbeResult, DbOperationError>;
}
