use async_trait::async_trait;

use crate::domain::{QueryResult, mysql_sql::MySqlStatement};

use super::{AccessMode, DbOperationError};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MySqlQueryExecutor: Send + Sync {
    async fn execute_adhoc_with_classified_statements(
        &self,
        dsn: &str,
        query: &str,
        statements: &[MySqlStatement],
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError>;
}
