use async_trait::async_trait;

use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SqliteDiagnosticsProvider: Send + Sync {
    async fn fetch_core_diagnostics(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError>;

    async fn fetch_quick_check(&self, endpoint: &ConnectionEndpoint) -> DiagnosticField;
}
