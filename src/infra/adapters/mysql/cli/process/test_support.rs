use std::path::Path;
use std::time::Duration;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::mysql_sql::MySqlStatement;

pub(in crate::adapters::mysql) async fn run_mysql_adhoc_with_timeout_for_test(
    option_file: &Path,
    statements: &[MySqlStatement],
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    super::adhoc::run_mysql_adhoc_with_program_and_statements(
        std::ffi::OsStr::new("mysql"),
        option_file,
        statements,
        AccessMode::ReadWrite,
        execution_timeout,
    )
    .await
    .map(|_| ())
}
