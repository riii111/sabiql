use crate::domain::connection::MySqlConnectionConfig;
#[cfg(unix)]
use std::ffi::OsStr;
use std::io;
use std::process::Stdio;
use std::time::Duration;

use crate::app::ports::outbound::{AccessMode, DatabaseCli, DbOperationError};
use tokio::process::Command;
use tokio::time::timeout;

use super::cli::process::test_support::run_mysql_adhoc_with_timeout_for_test;
use super::cli::{run_mysql_adhoc, sanitize_mysql_command_environment, validate_mysql_multi_query};
use super::dsn::NormalizedMySqlConfig;
use super::option_file::MySqlOptionFile;

#[cfg(unix)]
use super::cli::MYSQL_QUERY_TIMEOUT;
#[cfg(unix)]
use super::cli::test_support::run_mysql_cli_script_with_program;

#[doc(hidden)]
/// Runs a one-shot MySQL CLI query while owning the secure option file lifecycle.
pub async fn run_mysql_cli_query_for_test(
    config: &MySqlConnectionConfig,
    query: &str,
) -> Result<String, DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let args = [
        format!("--defaults-file={}", option_file.path.display()),
        "--no-login-paths".to_string(),
        "--connect-timeout=10".to_string(),
        "--skip-reconnect".to_string(),
        "--batch".to_string(),
        "--raw".to_string(),
        "--skip-column-names".to_string(),
        "--binary-mode".to_string(),
        format!("--execute={query}"),
    ];
    let mut command = Command::new("mysql");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    sanitize_mysql_command_environment(&mut command);
    let output = match timeout(Duration::from_secs(11), command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Err(DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                details: error.to_string(),
            });
        }
        Ok(Err(error)) => return Err(DbOperationError::ConnectionFailed(error.to_string())),
        Err(_) => {
            return Err(DbOperationError::Timeout(
                "mysql test query exceeded the connection timeout".to_string(),
            ));
        }
    };
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
    config: &MySqlConnectionConfig,
    query: &str,
) -> Result<(), DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    let statements =
        validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadWrite)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_adhoc(&option_file.path, &statements, AccessMode::ReadOnly)
        .await
        .map(|_| ())
}

#[doc(hidden)]
pub async fn execute_mysql_adhoc_with_timeout_for_test(
    config: &MySqlConnectionConfig,
    query: &str,
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    let statements =
        validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadWrite)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_adhoc_with_timeout_for_test(&option_file.path, &statements, execution_timeout).await
}

#[cfg(unix)]
#[doc(hidden)]
pub async fn run_mysql_cli_script_for_test(
    config: &MySqlConnectionConfig,
    script: &str,
) -> Result<Vec<u8>, DbOperationError> {
    let target = NormalizedMySqlConfig::new(config)?;
    let option_file = MySqlOptionFile::create(&target)?;
    run_mysql_cli_script_with_program(
        OsStr::new("mysql"),
        &option_file.path,
        script,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}
