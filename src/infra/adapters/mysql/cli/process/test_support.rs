use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::mysql_sql::MySqlStatement;

pub(in crate::adapters::mysql) async fn run_mysql_adhoc_with_timeout_for_test(
    program: &OsStr,
    option_file: &Path,
    statements: &[MySqlStatement],
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    super::adhoc::run_mysql_adhoc_with_program_and_statements_and_expected_columns(
        program,
        option_file,
        statements,
        access_mode,
        None,
        execution_timeout,
    )
    .await
    .map(|_| ())
}
