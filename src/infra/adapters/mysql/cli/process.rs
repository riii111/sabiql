use std::ffi::OsStr;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

#[cfg(unix)]
use std::io;
#[cfg(unix)]
use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
#[cfg(not(unix))]
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{DatabaseDiagnostic, RefreshScope};

use super::args::{MYSQL_CLIENT_MAX_PACKET_BYTES, mysql_adhoc_args, mysql_query_args};
#[cfg(not(unix))]
use super::error::classify_mysql_query_failure;
use super::error::{
    classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error, map_mysql_cli_spawn_error,
};
#[cfg(not(unix))]
use super::pipe::read_one_mysql_resultset_from_pipes;
use super::policy::{
    MYSQL_SESSION_MARKER_COLUMN, MYSQL_SESSION_SQL_MODE_COLUMN, query_failed_after_change,
    validate_mysql_session,
};
#[cfg(unix)]
use super::pty::{
    MySqlPty, create_mysql_pty, read_one_pty_resultset, read_one_pty_resultset_with_diagnostics,
    read_pty_all, read_pty_until_idle,
};
use super::sanitize_mysql_command_environment;
use super::xml::{MySqlResultSetFrameScanner, parse_mysql_xml, trace_mysql_statement};

mod session;
pub(in crate::adapters::mysql) use session::MySqlMetadataSession;
mod adhoc;
pub(in crate::adapters::mysql) use adhoc::run_mysql_adhoc;
mod single;
#[cfg(feature = "test-support")]
pub(in crate::adapters::mysql) mod test_support;
pub(in crate::adapters::mysql) use single::run_mysql_single_statement;
pub(super) mod metadata;

pub(in crate::adapters::mysql) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
#[cfg(unix)]
const MYSQL_PTY_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const MYSQL_SESSION_SETTINGS: &str = "SET SESSION autocommit=1, completion_type=NO_CHAIN";
const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

fn mysql_session_probe_query(marker: &str) -> String {
    format!(
        "SELECT '{marker}' AS {MYSQL_SESSION_MARKER_COLUMN}, @@SESSION.sql_mode AS {MYSQL_SESSION_SQL_MODE_COLUMN}"
    )
}

pub(in crate::adapters::mysql) struct MySqlProcess {
    pub(super) child: Child,
    pub(super) client_packet_limit_bytes: Option<usize>,
    pub(super) preview_byte_budget: bool,
    #[cfg(unix)]
    pub(super) pty: MySqlPty,
    #[cfg(not(unix))]
    pub(super) stdin: Option<ChildStdin>,
    #[cfg(not(unix))]
    pub(super) stdout: ChildStdout,
    #[cfg(not(unix))]
    pub(super) stderr: ChildStderr,
    #[cfg(not(unix))]
    pub(super) pending: Vec<u8>,
    #[cfg(not(unix))]
    pub(super) pending_stderr: Vec<u8>,
    #[cfg(not(unix))]
    pub(super) frame_scanner: MySqlResultSetFrameScanner,
}

impl MySqlProcess {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_query_args(program, mysql_query_args(option_file))
    }

    pub(in crate::adapters::mysql) fn spawn_with_adhoc_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_query_args(program, mysql_adhoc_args(option_file))
    }

    pub(in crate::adapters::mysql) fn spawn_with_query_args(
        program: &OsStr,
        args: Vec<String>,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_args_and_limits(program, args, Some(MYSQL_CLIENT_MAX_PACKET_BYTES), false)
    }

    pub(in crate::adapters::mysql) fn spawn_with_preview_program(
        program: &OsStr,
        args: Vec<String>,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_args_and_limits(program, args, Some(MYSQL_CLIENT_MAX_PACKET_BYTES), true)
    }

    pub(in crate::adapters::mysql) fn spawn_with_args(
        program: &OsStr,
        args: Vec<String>,
    ) -> Result<Self, DbOperationError> {
        Self::spawn_with_args_and_limits(program, args, None, false)
    }

    fn spawn_with_args_and_limits(
        program: &OsStr,
        args: Vec<String>,
        client_packet_limit_bytes: Option<usize>,
        preview_byte_budget: bool,
    ) -> Result<Self, DbOperationError> {
        #[cfg(unix)]
        {
            Self::spawn_with_pty(
                program,
                args,
                client_packet_limit_bytes,
                preview_byte_budget,
            )
        }

        #[cfg(not(unix))]
        {
            let mut command = Command::new(program);
            command
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            sanitize_mysql_command_environment(&mut command);
            let mut child = command.spawn().map_err(map_mysql_cli_spawn_error)?;
            let stdin = child.stdin.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stdin was not piped".to_string())
            })?;
            let stdout = child.stdout.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stdout was not piped".to_string())
            })?;
            let stderr = child.stderr.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stderr was not piped".to_string())
            })?;
            Ok(Self {
                child,
                client_packet_limit_bytes,
                preview_byte_budget,
                stdin: Some(stdin),
                stdout,
                stderr,
                pending: Vec::new(),
                pending_stderr: Vec::new(),
                frame_scanner: MySqlResultSetFrameScanner::default(),
            })
        }
    }

    #[cfg(unix)]
    fn spawn_with_pty(
        program: &OsStr,
        args: Vec<String>,
        client_packet_limit_bytes: Option<usize>,
        preview_byte_budget: bool,
    ) -> Result<Self, DbOperationError> {
        let (master, slave) = create_mysql_pty().map_err(|error| {
            DbOperationError::ConnectionFailed(format!("Unable to create MySQL PTY: {error}"))
        })?;
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stdout(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stderr(Stdio::from(slave))
            .kill_on_drop(true);
        sanitize_mysql_command_environment(&mut command);
        let child = command.spawn().map_err(map_mysql_cli_spawn_error)?;
        let output = TokioFile::from_std(
            master
                .try_clone()
                .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?,
        );
        let input = TokioFile::from_std(master);
        Ok(Self {
            child,
            client_packet_limit_bytes,
            preview_byte_budget,
            pty: MySqlPty {
                input,
                output,
                pending: Vec::new(),
                frame_scanner: MySqlResultSetFrameScanner::default(),
            },
        })
    }
}

#[cfg(unix)]
pub(super) async fn stop_mysql_process(
    child: &mut Child,
) -> Result<(ExitStatus, bool), DbOperationError> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?
    {
        return Ok((status, false));
    }
    let kill_result = child.kill().await;
    finish_mysql_process_stop(child, kill_result).await
}

#[cfg(unix)]
async fn finish_mysql_process_stop(
    child: &mut Child,
    kill_result: io::Result<()>,
) -> Result<(ExitStatus, bool), DbOperationError> {
    let status = child
        .wait()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, kill_result.is_ok()))
}

pub(super) async fn read_all_bytes<R>(reader: &mut R) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await?;
    Ok(output)
}

async fn finish_mysql_pipe<O, E>(
    stdout: &mut O,
    stderr: &mut E,
    child: &mut Child,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), DbOperationError>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    let (stdout, stderr, status) =
        tokio::join!(read_all_bytes(stdout), read_all_bytes(stderr), child.wait());
    let stdout = stdout.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let status = status.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, stdout, stderr))
}

pub(super) struct MySqlSessionResult {
    pub(super) status: ExitStatus,
    pub(super) forcibly_stopped: bool,
    #[cfg(not(unix))]
    pub(super) stdout: Vec<u8>,
    pub(super) error_bytes: Vec<u8>,
}

pub(super) fn validate_mysql_session_exit(
    result: &MySqlSessionResult,
    client_packet_limit_bytes: Option<usize>,
) -> Result<(), DbOperationError> {
    if has_mysql_cli_error(&result.error_bytes) {
        return Err(classify_mysql_query_failure_with_packet_limit(
            &result.error_bytes,
            client_packet_limit_bytes,
        ));
    }
    if !result.status.success() && !result.forcibly_stopped {
        return Err(classify_mysql_query_failure_with_packet_limit(
            &result.error_bytes,
            client_packet_limit_bytes,
        ));
    }
    Ok(())
}

async fn shutdown_mysql_input(process: &mut MySqlProcess) -> Result<(), DbOperationError> {
    #[cfg(unix)]
    {
        write_mysql_input(process, b"\x04").await?;
    }

    #[cfg(not(unix))]
    if let Some(mut stdin) = process.stdin.take() {
        stdin
            .shutdown()
            .await
            .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    }
    Ok(())
}

pub(in crate::adapters::mysql::cli) async fn finish_mysql_session(
    process: &mut MySqlProcess,
) -> Result<MySqlSessionResult, DbOperationError> {
    shutdown_mysql_input(process).await?;

    #[cfg(unix)]
    let error_bytes = read_pty_until_idle(&mut process.pty)
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

    #[cfg(not(unix))]
    let (stdout, error_bytes, status) = {
        let (status, stdout, stderr) =
            finish_mysql_pipe(&mut process.stdout, &mut process.stderr, &mut process.child).await?;
        let mut error_bytes = std::mem::take(&mut process.pending_stderr);
        error_bytes.extend_from_slice(&stderr);
        (stdout, error_bytes, status)
    };

    #[cfg(unix)]
    let (status, forcibly_stopped) = stop_mysql_process(&mut process.child).await?;
    #[cfg(not(unix))]
    let forcibly_stopped = false;

    Ok(MySqlSessionResult {
        status,
        forcibly_stopped,
        #[cfg(not(unix))]
        stdout,
        error_bytes,
    })
}

pub(in crate::adapters::mysql::cli) async fn finish_mysql_session_after_preview_frame(
    process: &mut MySqlProcess,
) -> Result<MySqlSessionResult, DbOperationError> {
    #[cfg(unix)]
    {
        shutdown_mysql_input(process).await?;
        // The caller has consumed the expected final resultset, so the client can be
        // terminated explicitly while the PTY is drained for already-produced diagnostics.
        let (error_bytes, status) = tokio::join!(
            read_pty_all(&mut process.pty),
            stop_mysql_process(&mut process.child),
        );
        let error_bytes =
            error_bytes.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let (status, forcibly_stopped) = status?;
        Ok(MySqlSessionResult {
            status,
            forcibly_stopped,
            error_bytes,
        })
    }

    #[cfg(not(unix))]
    finish_mysql_session(process).await
}

pub(super) async fn configure_mysql_session(
    process: &mut MySqlProcess,
    access_mode: AccessMode,
) -> Result<(), DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(process, MYSQL_SESSION_SETTINGS).await?;
    if access_mode.is_read_only() {
        write_mysql_statement(process, MYSQL_READ_ONLY_STATEMENT).await?;
    }
    write_mysql_statement(process, &mysql_session_probe_query(&marker)).await?;
    loop {
        let result = read_one_mysql_resultset(process).await?;
        let result = parse_mysql_xml(&result)?;
        if result.columns.is_empty() && result.values.is_empty() {
            continue;
        }
        return validate_mysql_session(&result, &marker);
    }
}

pub(super) async fn write_mysql_statement(
    process: &mut MySqlProcess,
    query: &str,
) -> Result<(), DbOperationError> {
    trace_mysql_statement(query.trim_end());
    write_mysql_input(process, &mysql_statement_input(query)).await
}

pub(super) async fn write_mysql_input(
    process: &mut MySqlProcess,
    input: &[u8],
) -> Result<(), DbOperationError> {
    #[cfg(unix)]
    let writer = &mut process.pty.input;
    #[cfg(not(unix))]
    let writer = process
        .stdin
        .as_mut()
        .ok_or_else(|| DbOperationError::ConnectionLost("mysql stdin was closed".to_string()))?;
    writer
        .write_all(input)
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok(())
}

pub(super) async fn cleanup_mysql_process(process: &mut MySqlProcess) {
    let _ = process.child.kill().await;
    #[cfg(unix)]
    drain_mysql_pty(&mut process.pty).await;
    #[cfg(not(unix))]
    {
        drop(process.stdin.take());
        let _ = tokio::join!(
            read_all_bytes(&mut process.stdout),
            read_all_bytes(&mut process.stderr)
        );
    }
    let _ = process.child.wait().await;
}

#[cfg(unix)]
async fn drain_mysql_pty(pty: &mut MySqlPty) {
    let _ = tokio::time::timeout(MYSQL_PTY_DRAIN_TIMEOUT, read_pty_all(pty)).await;
}

pub(super) async fn run_mysql_process_with_timeout<T, F>(
    execution_timeout: Duration,
    process: &mut MySqlProcess,
    possible_refresh_scope: RefreshScope,
    execute: F,
) -> Result<T, DbOperationError>
where
    F: AsyncFnOnce(&mut MySqlProcess) -> Result<T, DbOperationError>,
{
    match tokio::time::timeout(execution_timeout, execute(process)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            cleanup_mysql_process(process).await;
            Err(query_failed_after_change(error, possible_refresh_scope))
        }
        Err(_) => {
            cleanup_mysql_process(process).await;
            Err(query_failed_after_change(
                DbOperationError::Timeout("mysql query exceeded the execution timeout".to_string()),
                possible_refresh_scope,
            ))
        }
    }
}

pub(super) async fn read_one_mysql_resultset(
    process: &mut MySqlProcess,
) -> Result<Vec<u8>, DbOperationError> {
    #[cfg(unix)]
    {
        return read_one_pty_resultset(
            &mut process.pty,
            process.client_packet_limit_bytes,
            process.preview_byte_budget,
        )
        .await;
    }
    #[cfg(not(unix))]
    read_one_mysql_resultset_from_pipes(
        &mut process.stdout,
        &mut process.stderr,
        &mut process.child,
        &mut process.pending,
        &mut process.pending_stderr,
        &mut process.frame_scanner,
        process.client_packet_limit_bytes,
        process.preview_byte_budget,
    )
    .await
}

pub(super) async fn read_one_mysql_resultset_with_diagnostics(
    process: &mut MySqlProcess,
) -> Result<(Vec<u8>, Vec<DatabaseDiagnostic>), DbOperationError> {
    #[cfg(unix)]
    {
        return read_one_pty_resultset_with_diagnostics(
            &mut process.pty,
            process.client_packet_limit_bytes,
            process.preview_byte_budget,
        )
        .await;
    }
    #[cfg(not(unix))]
    super::pipe::read_one_mysql_resultset_from_pipes_with_diagnostics(
        &mut process.stdout,
        &mut process.stderr,
        &mut process.child,
        &mut process.pending,
        &mut process.pending_stderr,
        &mut process.frame_scanner,
        process.client_packet_limit_bytes,
        process.preview_byte_budget,
    )
    .await
}

fn mysql_statement_input(query: &str) -> Vec<u8> {
    let query = query.trim_end();
    [query.as_bytes(), b"\n;\n"].concat()
}

#[cfg(test)]
mod process_tests;

#[cfg(all(test, unix))]
mod tests;

#[cfg(all(test, not(unix)))]
mod windows_tests;
