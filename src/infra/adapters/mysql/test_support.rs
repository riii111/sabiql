use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::csv_export::export_to_path;
use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::cli::{
    export_mysql_csv_to_file, run_mysql_adhoc,
    run_mysql_adhoc_with_program_and_statements_and_expected_columns,
    run_mysql_command_with_timeout, validate_mysql_multi_query,
};
use super::dsn::parse_and_validate_mysql_dsn;
use super::option_file::MySqlOptionFile;

#[cfg(unix)]
use super::cli::MYSQL_QUERY_TIMEOUT;
#[cfg(unix)]
use super::cli::test_support::run_mysql_cli_script_with_program;

#[doc(hidden)]
/// Runs a one-shot MySQL CLI query while owning the secure option file lifecycle.
pub async fn run_mysql_cli_query_for_test(
    dsn: &str,
    query: &str,
) -> Result<String, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let args = [
        format!("--defaults-file={}", option_file.path.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--batch".to_string(),
        "--raw".to_string(),
        "--skip-column-names".to_string(),
        "--binary-mode".to_string(),
        "--skip-reconnect".to_string(),
        format!("--execute={query}"),
    ];
    let output = run_mysql_command_with_timeout(
        args,
        Some(&option_file.path),
        Duration::from_secs(11),
        "mysql test query exceeded the connection timeout",
    )
    .await?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DbOperationError::QueryFailed(if details.is_empty() {
            "mysql test query failed".to_string()
        } else {
            details
        }));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
}

#[doc(hidden)]
/// Runs an adhoc query with a read-only MySQL session so integration tests can verify server-side
/// rejection of a side effect without the app-side policy gate.
pub async fn execute_mysql_adhoc_with_read_only_session_for_test(
    dsn: &str,
    query: &str,
) -> Result<(), DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let statements =
        validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadWrite)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_adhoc(&option_file.path, &statements, AccessMode::ReadOnly)
        .await
        .map(|_| ())
}

#[doc(hidden)]
pub async fn execute_mysql_adhoc_with_timeout_for_test(
    dsn: &str,
    query: &str,
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let statements =
        validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadWrite)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_adhoc_with_program_and_statements_and_expected_columns(
        OsStr::new("mysql"),
        &option_file.path,
        &statements,
        AccessMode::ReadWrite,
        None,
        execution_timeout,
    )
    .await
    .map(|_| ())
}

#[cfg(unix)]
#[doc(hidden)]
pub async fn run_mysql_cli_script_for_test(
    dsn: &str,
    script: &str,
) -> Result<Vec<u8>, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_cli_script_with_program(
        OsStr::new("mysql"),
        &option_file.path,
        script,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

#[doc(hidden)]
/// Runs CSV export without client-side query policy validation so integration tests can verify
/// the MySQL read-only session at the server boundary.
pub async fn export_mysql_csv_to_path_for_test(
    dsn: &str,
    query: &str,
    path: PathBuf,
) -> Result<PathBuf, DbOperationError> {
    let target = parse_and_validate_mysql_dsn(dsn)?;
    let query = query.to_string();
    export_to_path(path, move |temporary_path| async move {
        export_mysql_csv_to_file(target, &query, temporary_path).await
    })
    .await
}
