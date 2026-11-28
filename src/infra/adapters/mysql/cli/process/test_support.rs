use std::path::Path;
use std::time::Duration;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::mysql_sql::MySqlStatement;

#[cfg(unix)]
impl super::MySqlProcess {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &std::ffi::OsStr,
        option_file: &Path,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_query_args(program, super::super::args::mysql_query_args(option_file))
    }
}

pub(in crate::adapters::mysql) async fn run_mysql_adhoc_with_timeout_for_test(
    option_file: &Path,
    statements: &[MySqlStatement],
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    super::adhoc::test_support::run_mysql_adhoc_with_program_and_statements(
        std::ffi::OsStr::new("mysql"),
        option_file,
        statements,
        AccessMode::ReadWrite,
        execution_timeout,
    )
    .await
    .map(|_| ())
}
