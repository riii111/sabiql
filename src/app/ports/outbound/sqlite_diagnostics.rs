use async_trait::async_trait;

use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};

use super::DbOperationError;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SqliteDiagnosticsProvider: Send + Sync {
    async fn fetch_diagnostics_core(
        &self,
        dsn: &str,
        read_only: bool,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError>;

    async fn fetch_quick_check(&self, dsn: &str, read_only: bool) -> DiagnosticField;
}
