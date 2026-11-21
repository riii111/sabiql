use async_trait::async_trait;

use crate::domain::connection::ConnectionEndpoint;

use super::DbOperationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MySqlConnectionProbeResult {
    pub lower_case_table_names: u8,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MySqlConnectionProbe: Send + Sync {
    async fn probe(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<MySqlConnectionProbeResult, DbOperationError>;
}
