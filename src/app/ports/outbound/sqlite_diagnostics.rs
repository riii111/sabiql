use async_trait::async_trait;

use crate::domain::SqliteDiagnosticsSnapshot;

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SqliteDiagnosticsProvider: Send + Sync {
    async fn fetch_diagnostics(
        &self,
        dsn: &str,
        read_only: bool,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError>;
}
