use std::ffi::OsStr;
use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

#[cfg(unix)]
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
#[cfg(not(unix))]
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DatabaseCli, DbOperationError};

use super::args::mysql_query_args;
use super::error::{classify_mysql_query_failure, has_mysql_cli_error, validate_mode_probe};
#[cfg(not(unix))]
use super::pipe::{read_all, read_one_mysql_resultset_from_pipes};
use super::policy::{MYSQL_SESSION_MARKER_COLUMN, validate_mysql_session_marker};
#[cfg(all(unix, feature = "test-support"))]
use super::pty::read_pty_until_first_byte_then_idle;
#[cfg(unix)]
use super::pty::{
    MySqlPty, create_mysql_pty, read_one_pty_resultset, read_pty_all, read_pty_until_idle,
};
#[cfg(all(unix, feature = "test-support"))]
use super::xml::trace_mysql_frame;
use super::xml::{MySqlResultsetFrameScanner, parse_mysql_xml, trace_mysql_statement};

mod session;
pub(in crate::adapters::mysql) use session::MySqlMetadataSession;
mod adhoc;
pub(in crate::adapters::mysql) use adhoc::run_mysql_adhoc;
#[cfg(feature = "test-support")]
pub(in crate::adapters::mysql) use adhoc::run_mysql_adhoc_with_timeout_for_test;
mod single;
pub(in crate::adapters::mysql) use single::run_mysql_single_statement;
mod metadata;
pub(in crate::adapters::mysql) use metadata::mysql_metadata_columns;

#[cfg(all(unix, feature = "test-support"))]
use super::super::dsn::parse_and_validate_mysql_dsn;
#[cfg(all(unix, feature = "test-support"))]
use super::super::option_file::MySqlOptionFile;

pub(in crate::adapters::mysql) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
#[cfg(unix)]
const MYSQL_PTY_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

pub(in crate::adapters::mysql) struct MySqlProcess {
    pub(super) child: Child,
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
    pub(super) frame_scanner: MySqlResultsetFrameScanner,
}

impl MySqlProcess {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        #[cfg(unix)]
        {
            Self::spawn_with_pty(program, option_file)
        }

        #[cfg(not(unix))]
        {
            let mut command = Command::new(program);
            command
                .args(mysql_query_args(option_file))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env_remove("MYSQL_PWD")
                .env_remove("MYSQL_PASSWORD")
                .kill_on_drop(true);
            let mut child = command.spawn().map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    DbOperationError::CommandNotFound {
                        command: DatabaseCli::MySql,
                        details: error.to_string(),
                    }
                } else {
                    DbOperationError::ConnectionFailed(error.to_string())
                }
            })?;
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
                stdin: Some(stdin),
                stdout,
                stderr,
                pending: Vec::new(),
                pending_stderr: Vec::new(),
                frame_scanner: MySqlResultsetFrameScanner::default(),
            })
        }
    }

    #[cfg(unix)]
    fn spawn_with_pty(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        let (master, slave) = create_mysql_pty().map_err(|error| {
            DbOperationError::ConnectionFailed(format!("Unable to create MySQL PTY: {error}"))
        })?;
        let mut command = Command::new(program);
        command
            .args(mysql_query_args(option_file))
            .stdin(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stdout(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stderr(Stdio::from(slave))
            .env_remove("MYSQL_PWD")
            .env_remove("MYSQL_PASSWORD")
            .kill_on_drop(true);
        let child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                DbOperationError::CommandNotFound {
                    command: DatabaseCli::MySql,
                    details: error.to_string(),
                }
            } else {
                DbOperationError::ConnectionFailed(error.to_string())
            }
        })?;
        let output = TokioFile::from_std(
            master
                .try_clone()
                .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?,
        );
        let input = TokioFile::from_std(master);
        Ok(Self {
            child,
            pty: MySqlPty {
                input,
                output,
                pending: Vec::new(),
                frame_scanner: MySqlResultsetFrameScanner::default(),
            },
        })
    }
}

#[cfg(unix)]
async fn stop_mysql_process(child: &mut Child) -> Result<(ExitStatus, bool), DbOperationError> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?
    {
        return Ok((status, false));
    }
    let _ = child.kill().await;
    let status = child
        .wait()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, true))
}

pub(super) struct MySqlSessionResult {
    pub(super) status: ExitStatus,
    pub(super) forcibly_stopped: bool,
    #[cfg(not(unix))]
    pub(super) stdout: Vec<u8>,
    pub(super) error_bytes: Vec<u8>,
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
        let (stdout, stderr, status) = tokio::join!(
            read_all(&mut process.stdout),
            read_all(&mut process.stderr),
            process.child.wait()
        );
        let stdout = stdout.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let status = status.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
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
    if !access_mode.is_read_only() {
        return Ok(());
    }

    let marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(process, MYSQL_READ_ONLY_STATEMENT).await?;
    write_mysql_statement(
        process,
        &format!("SELECT '{marker}' AS {MYSQL_SESSION_MARKER_COLUMN}"),
    )
    .await?;
    loop {
        let result = read_one_mysql_resultset(process).await?;
        let result = parse_mysql_xml(&result)?;
        if result.columns.is_empty() && result.values.is_empty() {
            continue;
        }
        return validate_mysql_session_marker(&result, &marker);
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
    process
        .pty
        .input
        .write_all(input)
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(not(unix))]
    process
        .stdin
        .as_mut()
        .ok_or_else(|| DbOperationError::ConnectionLost("mysql stdin was closed".to_string()))?
        .write_all(input)
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(unix)]
    process
        .pty
        .input
        .flush()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(not(unix))]
    process
        .stdin
        .as_mut()
        .ok_or_else(|| DbOperationError::ConnectionLost("mysql stdin was closed".to_string()))?
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
        let _ = tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
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
    execute: F,
) -> Result<T, DbOperationError>
where
    F: AsyncFnOnce(&mut MySqlProcess) -> Result<T, DbOperationError>,
{
    match tokio::time::timeout(execution_timeout, execute(process)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            cleanup_mysql_process(process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_process(process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod timeout_tests {
    use std::time::Duration;

    use super::MYSQL_QUERY_TIMEOUT;

    #[test]
    fn keeps_production_query_timeout_at_31_seconds() {
        assert_eq!(MYSQL_QUERY_TIMEOUT, Duration::from_secs(31));
    }
}

pub(super) async fn read_one_mysql_resultset(
    process: &mut MySqlProcess,
) -> Result<Vec<u8>, DbOperationError> {
    #[cfg(unix)]
    {
        return read_one_pty_resultset(&mut process.pty).await;
    }
    #[cfg(not(unix))]
    read_one_mysql_resultset_from_pipes(
        &mut process.stdout,
        &mut process.stderr,
        &mut process.child,
        &mut process.pending,
        &mut process.pending_stderr,
        &mut process.frame_scanner,
    )
    .await
}

fn mysql_statement_input(query: &str) -> Vec<u8> {
    let query = query.trim_end();
    [query.as_bytes(), b"\n;\n"].concat()
}

#[cfg(test)]
mod statement_input_tests {
    use super::mysql_statement_input;

    #[test]
    fn separates_statements_after_line_comments_with_semicolons() {
        for query in [
            "SELECT 1",
            "SELECT 1 -- trailing comment;",
            "SELECT 1 # trailing comment;",
        ] {
            assert_eq!(
                mysql_statement_input(query),
                format!("{query}\n;\n").into_bytes()
            );
        }
    }

    #[test]
    fn adds_an_independent_separator_after_an_existing_semicolon() {
        assert_eq!(mysql_statement_input("SELECT 1;"), b"SELECT 1;\n;\n");
    }
}

#[cfg(all(unix, feature = "test-support"))]
pub(in crate::adapters::mysql) async fn run_mysql_cli_script_for_test(
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

#[cfg(all(unix, feature = "test-support"))]
async fn run_mysql_cli_script_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    script: &str,
    first_byte_timeout: Duration,
) -> Result<Vec<u8>, DbOperationError> {
    let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
    run_mysql_cli_script_process(&mut process, script, first_byte_timeout).await
}

#[cfg(all(test, unix, feature = "test-support"))]
async fn run_mysql_cli_script_with_program_and_pid(
    program: &OsStr,
    option_file: &std::path::Path,
    script: &str,
    first_byte_timeout: Duration,
) -> Result<(u32, Result<Vec<u8>, DbOperationError>), DbOperationError> {
    let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
    let pid = process.child.id().ok_or_else(|| {
        DbOperationError::ConnectionLost("mysql child exited before cleanup tracking".to_string())
    })?;
    let result = run_mysql_cli_script_process(&mut process, script, first_byte_timeout).await;
    Ok((pid, result))
}

#[cfg(all(unix, feature = "test-support"))]
async fn run_mysql_cli_script_process(
    process: &mut MySqlProcess,
    script: &str,
    first_byte_timeout: Duration,
) -> Result<Vec<u8>, DbOperationError> {
    let result = async {
        trace_mysql_statement(script);
        write_mysql_input(process, script.as_bytes()).await?;
        write_mysql_input(process, b"\x04").await?;
        let output = read_pty_until_first_byte_then_idle(&mut process.pty, first_byte_timeout)
            .await
            .map_err(|error| {
                if error.kind() == io::ErrorKind::TimedOut {
                    DbOperationError::Timeout(error.to_string())
                } else {
                    DbOperationError::QueryFailed(error.to_string())
                }
            })?;
        trace_mysql_frame("receive script output", output.len());
        Ok(output)
    }
    .await;
    if result.is_err() {
        cleanup_mysql_process(process).await;
    } else {
        let _ = stop_mysql_process(&mut process.child).await;
    }
    result
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::super::export::run_mysql_export_process;
    use super::super::xml::MySqlResultSet;
    use super::adhoc::run_mysql_adhoc_with_program_and_statements_and_expected_columns;
    use super::metadata::{
        mysql_metadata_columns_external_with_program,
        run_mysql_metadata_query_with_read_only_session_with_timeout,
    };
    use super::single::run_mysql_single_statement_process;
    use super::*;
    use crate::adapters::csv_export::export_to_path;
    use crate::domain::mysql_sql::{classify_mysql_statement, split_mysql_statements};
    use crate::domain::{CommandTag, QueryValue, RefreshScope};

    mod cleanup {
        use super::*;

        #[tokio::test]
        async fn bounds_pty_drain_when_the_slave_stays_open() {
            let (master, _slave) = create_mysql_pty().expect("create test PTY");
            let mut pty = MySqlPty {
                input: TokioFile::from_std(master.try_clone().expect("clone PTY master")),
                output: TokioFile::from_std(master),
                pending: Vec::new(),
                frame_scanner: MySqlResultsetFrameScanner::default(),
            };

            assert!(
                tokio::time::timeout(
                    MYSQL_PTY_DRAIN_TIMEOUT + Duration::from_secs(1),
                    drain_mysql_pty(&mut pty),
                )
                .await
                .is_ok()
            );
        }
    }

    async fn export_mysql_csv_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        path: PathBuf,
        execution_timeout: Duration,
    ) -> Result<(), DbOperationError> {
        let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
        run_mysql_process_with_timeout(execution_timeout, &mut process, async |process| {
            run_mysql_export_process(process, option_file, query, path).await
        })
        .await
    }
    async fn run_mysql_single_statement_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        access_mode: AccessMode,
        execution_timeout: Duration,
    ) -> Result<MySqlResultSet, DbOperationError> {
        let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
        run_mysql_process_with_timeout(execution_timeout, &mut process, async |process| {
            run_mysql_single_statement_process(process, query, access_mode).await
        })
        .await
    }

    fn fake_mysql(mode: &str) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let probe_response = match mode {
            "missing" => "exit 0".to_string(),
            "invalid" => {
                "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
                    .to_string()
            }
            "unsupported" => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_probe\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">ANSI_QUOTES</field></row></resultset>'".to_string(),
            "timeout" => "while :; do :; done".to_string(),
            _ => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_probe\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">STRICT_TRANS_TABLES</field></row></resultset>'".to_string(),
        };
        let user_response = if mode == "failure" {
            "printf '%s\\n' '<resultset><row><field name=\"partial\">row</field></row></resultset>'\n    printf '%s\\n' 'ERROR 1064 (42000): syntax error' >&2\n    exit 1"
        } else if mode == "no_result_failure" {
            "printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2\n    exit 1"
        } else if mode == "connection_refused" {
            "printf '%s\\n' \"ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (111)\" >&2\n    exit 1"
        } else if mode == "field_error" {
            "printf '%s\\n' '<resultset><row><field name=\"message\">line 1
ERROR 1146 (42S02): this is a cell value</field></row></resultset>'"
        } else {
            "printf '%s\\n' '<resultset><row><field name=\"value\">ok</field></row></resultset>'"
        };
        let session_failure = if mode == "read_only_failure" {
            "printf '%s\\n' 'ERROR 1227 (42000): access denied to set transaction read only' >&2\n      exit 1"
        } else {
            ""
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
phase=probe
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = "$eof" ] && exit 0
  [ "$line" = ";" ] && continue
  if [ "$phase" = probe ]; then
    marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
    {probe_response}
    phase=user
  else
    case "$line" in
      "SET SESSION TRANSACTION READ ONLY")
        {session_failure}
        ;;
      *__sabiql_session_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
        ;;
      *)
        {user_response}
        exit 0
        ;;
    esac
  fi
done
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, log_file)
    }

    #[cfg(feature = "test-support")]
    fn fake_mysql_without_output() -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let script = r"#!/bin/sh
while :; do :; done
";
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, option_file)
    }

    fn fake_mysql_multi() -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(false, None, false)
    }

    fn fake_mysql_multi_with_marker_failure() -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(true, None, false)
    }

    fn fake_mysql_multi_with_statement_failure(error: &str) -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(false, Some(error), false)
    }

    fn fake_mysql_multi_with_tail_failure() -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(false, None, true)
    }

    fn fake_mysql_metadata_columns(fail_read_only: bool) -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_metadata_columns_with_hanging_query(fail_read_only, false)
    }

    fn fake_mysql_metadata_columns_with_hanging_query(
        fail_read_only: bool,
        hang_after_query: bool,
    ) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let read_only_failure = if fail_read_only {
            "printf '%s\\n' 'ERROR 1227 (42000): access denied to set transaction read only' >&2\n      exit 1"
        } else {
            ""
        };
        let query_tail = if hang_after_query {
            "while :; do :; done"
        } else {
            ""
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      printf '%s\t%s\n' '__sabiql_probe' '__sabiql_sql_mode'
      printf '%s\t%s\n' "$marker" 'STRICT_TRANS_TABLES'
      ;;
    *"SET SESSION TRANSACTION READ ONLY"*)
      {read_only_failure}
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\n' '__sabiql_session_marker'
      printf '%s\n' "$marker"
      ;;
    *"SHOW DATABASES"*)
      printf '%s\n' 'Database'
      {query_tail}
      ;;
  esac
done
exit 0
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, option_file)
    }

    fn fake_mysql_multi_with_mode(
        marker_failure: bool,
        statement_error: Option<&str>,
        tail_error: bool,
    ) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let update_response = statement_error.map_or_else(
            || {
                "printf '%s\\n' '<resultset><row><field name=\"affected\">ok</field></row></resultset>'"
                    .to_string()
            },
            |error| format!("printf '%s\\n' '{error}' >&2"),
        );
        let tail = if tail_error {
            "printf '%s\\n' input_closed >> \"$log\"\nprintf '%s\\n' 'ERROR 1054 (42S02): tail error' >&2\n  exit 1"
        } else {
            "exit 0"
        };
        let tail_after_create = if tail_error {
            format!("if [ \"$last_statement\" = create ]; then\n        {tail}\n      fi")
        } else {
            String::new()
        };
        let marker_response = if marker_failure {
            "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
                .to_string()
        } else {
            format!("marker=$(printf '%s\\n' \"$line\" | sed \"s/.*SELECT '\\\\([^']*\\\\)' AS __sabiql_marker.*/\\\\1/\")
      rows=0
      case \"$line\" in *ROW_COUNT\\(\\)* ) rows=3 ;; esac
      printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field><field name=\"affected_rows\">'\"$rows\"'</field></row></resultset>'
      if [ \"$pending_error\" = 1 ]; then
        sleep 0.05
        printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2
        pending_error=0
      fi
      {tail_after_create}")
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
pending_error=0
last_statement=none
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *"$eof"*)
      {tail}
      ;;
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    "SET SESSION TRANSACTION READ ONLY")
      ;;
    ";")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      ;;
      *__sabiql_marker*)
      {marker_response}
      ;;
    *metadata_source*)
      printf '%s\n' 'ERROR 1060 (42S21): Duplicate column name duplicate_alias'
      printf '%s\n' '<resultset></resultset>'
      ;;
    *missing_column*)
      pending_error=1
      ;;
    *SELECT*)
      case "$line" in
        *WHERE\ FALSE*)
          printf '%s\n' '<resultset></resultset>'
          ;;
        *)
          value=one
          case "$line" in *SELECT\ 2*) value=two ;; esac
          printf '%s\n' '<resultset><row><field name="value">'"$value"'</field></row></resultset>'
          ;;
      esac
      ;;
    *UPDATE*)
      last_statement=update
      {update_response}
      ;;
    *CREATE*)
      last_statement=create
      printf '%s\n' '<resultset><row><field name="affected">ok</field></row></resultset>'
      ;;
    *)
      printf '%s\n' '<resultset></resultset>'
      ;;
  esac
done
{tail}
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, option_file)
    }

    mod single_statement {
        use super::*;

        #[tokio::test]
        async fn sends_user_sql_only_after_a_valid_mode_probe() {
            let (_directory, program, log_file) = fake_mysql("success");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            assert_eq!(result.columns, vec!["value"]);
            assert_eq!(result.values[0][0].as_str(), Some("ok"));
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.contains("__sabiql_probe"));
            assert!(log.contains("SELECT 123"));
            assert!(!log.contains(MYSQL_READ_ONLY_STATEMENT));
        }

        #[tokio::test]
        async fn read_only_session_failure_never_writes_user_sql() {
            let (_directory, program, log_file) = fake_mysql("read_only_failure");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadOnly,
                Duration::from_secs(5),
            )
            .await;

            assert!(result.is_err());
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
            assert!(!log.contains("SELECT 123"), "{log}");
        }

        #[tokio::test]
        async fn nonzero_cli_exit_discards_any_collected_stdout() {
            let (_directory, program, log_file) = fake_mysql("failure");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
        }

        #[tokio::test]
        async fn classifies_cli_error_when_no_resultset_is_emitted() {
            let (_directory, program, log_file) = fake_mysql("no_result_failure");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::ObjectMissing(details))
                    if details.contains("missing_column")
            ));
        }

        #[tokio::test]
        async fn classifies_connection_refusal_from_the_shared_cli_error_path() {
            let (_directory, program, log_file) = fake_mysql("connection_refused");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(result, Err(DbOperationError::ConnectionFailed(_))));
        }
    }

    mod adhoc {
        use super::*;

        #[tokio::test]
        async fn configures_read_only_session_before_user_sql() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let log_file = PathBuf::from(format!("{}.log", option_file.display()));
            let statements = split_mysql_statements("SELECT 2")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadOnly,
                None,
                Duration::from_secs(5),
            )
            .await
            .unwrap_or_else(|error| {
                let log = fs::read_to_string(&log_file).unwrap_or_default();
                panic!("read-only execution failed: {error:?}; log: {log}");
            });

            assert_eq!(
                result.result_set.unwrap().values[0][0].as_str(),
                Some("two")
            );
            let log = fs::read_to_string(log_file).unwrap();
            let session_index = log
                .find(MYSQL_READ_ONLY_STATEMENT)
                .expect("read-only session statement");
            let user_index = log.find("SELECT 2").expect("user statement");
            assert!(session_index < user_index, "{log}");
            assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
        }

        #[tokio::test]
        async fn known_empty_result_uses_expected_columns_without_replaying_query() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let query = "SELECT 1 AS first_alias WHERE FALSE";
            let statements = split_mysql_statements(query)
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadOnly,
                Some(&["first_alias"]),
                Duration::from_secs(5),
            )
            .await
            .expect("known empty result");
            let result_set = result.result_set.expect("result set");
            assert_eq!(result_set.columns, ["first_alias"]);
            assert!(result_set.values.is_empty());

            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert_eq!(log.matches(query).count(), 1, "{log}");
            assert!(!log.contains("__sabiql_metadata_inner"));
        }

        #[tokio::test]
        async fn duplicate_empty_select_columns_are_rejected_without_replaying_user_sql() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let query = "SELECT 1 AS duplicate_alias, 2 AS duplicate_alias WHERE FALSE";
            let statements = split_mysql_statements(query)
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await;

            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(
                matches!(
                    result,
                    Err(DbOperationError::UnsupportedOperation(ref details))
                        if details.contains("duplicate column names")
                ),
                "result={result:?}; log={log}"
            );
            assert_eq!(
                log.lines().filter(|line| *line == query).count(),
                1,
                "{log}"
            );
        }

        #[tokio::test]
        async fn generated_preview_and_metadata_queries_configure_read_only_session() {
            for query in [
                "SELECT id FROM app.items ORDER BY id LIMIT 10 OFFSET 0",
                "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES",
            ] {
                let (_directory, program, option_file) = fake_mysql_multi();
                let statements = split_mysql_statements(query)
                    .unwrap()
                    .into_iter()
                    .map(|sql| classify_mysql_statement(&sql).unwrap())
                    .collect::<Vec<_>>();

                run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                    OsStr::new(&program),
                    &option_file,
                    &statements,
                    AccessMode::ReadOnly,
                    None,
                    Duration::from_secs(5),
                )
                .await
                .unwrap();

                let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
                let session_index = log
                    .find(MYSQL_READ_ONLY_STATEMENT)
                    .expect("read-only session statement");
                let query_index = log.find(query).expect("generated query");
                assert!(session_index < query_index, "{query}: {log}");
            }
        }
    }

    mod metadata_session {
        use super::*;

        #[tokio::test]
        async fn reuses_one_process_for_ordered_resultsets() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let mut session =
                MySqlMetadataSession::spawn_with_program(OsStr::new(&program), &option_file)
                    .expect("spawn fake mysql");

            session.probe().await.expect("mode probe");
            session
                .prepare_read_only()
                .await
                .expect("read-only session setup");
            for query in [
                "SELECT TABLES",
                "SELECT COLUMNS",
                "SELECT INDEXES",
                "SELECT FOREIGN_KEYS",
                "SELECT TRIGGERS",
                "SHOW CREATE TABLE items",
            ] {
                session.execute(query).await.expect("metadata resultset");
            }
            let empty_result = session
                .execute_with_expected_columns("EMPTY_RESULT", &["known_column"])
                .await
                .expect("empty metadata resultset");
            assert_eq!(empty_result.columns, ["known_column"]);
            assert!(empty_result.values.is_empty());
            session.finish().await.expect("finish fake mysql");

            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert_eq!(
                log.lines()
                    .filter(|line| line.starts_with("process="))
                    .count(),
                1
            );
            let positions = [
                "__sabiql_probe",
                MYSQL_READ_ONLY_STATEMENT,
                MYSQL_SESSION_MARKER_COLUMN,
                "SELECT TABLES",
                "SELECT COLUMNS",
                "SELECT INDEXES",
                "SELECT FOREIGN_KEYS",
                "SELECT TRIGGERS",
                "SHOW CREATE TABLE items",
            ]
            .into_iter()
            .map(|query| log.find(query).expect("query in transcript"))
            .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
        }

        #[tokio::test]
        async fn external_metadata_fallback_configures_read_only_session_before_query() {
            let (_directory, program, option_file) = fake_mysql_metadata_columns(false);
            let columns = mysql_metadata_columns_external_with_program(
                OsStr::new(&program),
                &option_file,
                "SHOW DATABASES",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap_or_else(|error| {
                let log = fs::read_to_string(format!("{}.log", option_file.display()))
                    .unwrap_or_default();
                panic!("external metadata fallback failed: {error:?}; log: {log}");
            });

            assert_eq!(columns, ["Database"]);
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            let positions = [
                "__sabiql_probe",
                MYSQL_READ_ONLY_STATEMENT,
                MYSQL_SESSION_MARKER_COLUMN,
                "SHOW DATABASES",
            ]
            .into_iter()
            .map(|query| log.find(query).expect("query in transcript"))
            .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
        }

        #[tokio::test]
        async fn external_metadata_fallback_setup_failure_never_sends_query() {
            let (_directory, program, option_file) = fake_mysql_metadata_columns(true);
            let result = mysql_metadata_columns_external_with_program(
                OsStr::new(&program),
                &option_file,
                "SHOW DATABASES",
                AccessMode::ReadOnly,
            )
            .await;

            assert!(result.is_err());
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
            assert!(!log.contains("SHOW DATABASES"), "{log}");
        }

        #[tokio::test]
        async fn external_metadata_timeout_kills_and_reaps_the_process() {
            let (_directory, program, option_file) =
                fake_mysql_metadata_columns_with_hanging_query(false, true);
            let result = run_mysql_metadata_query_with_read_only_session_with_timeout(
                OsStr::new(&program),
                &option_file,
                "SHOW DATABASES",
                Duration::from_secs(10),
            )
            .await;

            assert!(matches!(result, Err(DbOperationError::Timeout(_))));
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            let pid = log
                .lines()
                .find_map(|line| line.strip_prefix("process=")?.parse::<i32>().ok())
                .expect("metadata process pid");
            let status = std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .expect("check metadata process");
            assert!(!status.success(), "metadata process {pid} is still running");
        }
    }

    mod export {
        use super::*;

        #[tokio::test]
        async fn exports_mysql_xml_rows_through_the_shared_csv_writer() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let path = option_file.with_file_name("export.csv");

            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 1",
                path.clone(),
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            assert_eq!(fs::read_to_string(path).unwrap(), "value\none\n");
        }

        #[tokio::test]
        async fn configures_read_only_session_before_user_sql() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let log_file = PathBuf::from(format!("{}.log", option_file.display()));
            let path = option_file.with_file_name("export.csv");

            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 1",
                path,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            let log = fs::read_to_string(log_file).unwrap();
            let session_index = log
                .find(MYSQL_READ_ONLY_STATEMENT)
                .expect("read-only session statement");
            let user_index = log.find("SELECT 1").expect("user statement");
            assert!(session_index < user_index, "{log}");
            assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
        }

        #[tokio::test]
        async fn ignores_cli_error_text_inside_resultset_fields() {
            let (_directory, program, option_file) = fake_mysql("field_error");
            let path = option_file.with_file_name("export.csv");

            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 1",
                path.clone(),
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            assert_eq!(
                fs::read_to_string(path).unwrap(),
                "message\n\"line 1\nERROR 1146 (42S02): this is a cell value\"\n"
            );
        }

        #[tokio::test]
        async fn read_only_session_failure_never_writes_user_sql_or_partial_file() {
            let (_directory, program, option_file) = fake_mysql("read_only_failure");
            let log_file = PathBuf::from(format!("{}.log", option_file.display()));
            let output_directory = tempfile::tempdir().unwrap();
            let final_path = output_directory.path().join("export.csv");

            let result = export_to_path(final_path.clone(), |path| {
                export_mysql_csv_with_program(
                    OsStr::new(&program),
                    &option_file,
                    "SELECT 123",
                    path,
                    Duration::from_secs(5),
                )
            })
            .await;

            assert!(result.is_err());
            let log = fs::read_to_string(log_file).unwrap();
            assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
            assert!(!log.contains("SELECT 123"), "{log}");
            assert!(!final_path.exists());
            assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
        }

        #[tokio::test]
        async fn failure_removes_the_partial_file() {
            let (_directory, program, option_file) = fake_mysql("failure");
            let output_directory = tempfile::tempdir().unwrap();
            let final_path = output_directory.path().join("export.csv");

            let result = export_to_path(final_path.clone(), |path| {
                export_mysql_csv_with_program(
                    OsStr::new(&program),
                    &option_file,
                    "SELECT 1",
                    path,
                    Duration::from_secs(5),
                )
            })
            .await;

            assert!(result.is_err());
            assert!(!final_path.exists());
            assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
        }

        #[tokio::test]
        async fn timeout_kills_the_process_and_removes_the_partial_file() {
            let (_directory, program, option_file) = fake_mysql("timeout");
            let output_directory = tempfile::tempdir().unwrap();
            let final_path = output_directory.path().join("export.csv");

            let result = export_to_path(final_path.clone(), |path| {
                export_mysql_csv_with_program(
                    OsStr::new(&program),
                    &option_file,
                    "SELECT 1",
                    path,
                    Duration::from_millis(50),
                )
            })
            .await;

            assert!(matches!(result, Err(DbOperationError::Timeout(_))));
            assert!(!final_path.exists());
            assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
        }
    }

    mod probe {
        use super::*;

        #[tokio::test]
        async fn failure_never_writes_user_sql() {
            for mode in ["unsupported", "invalid", "missing"] {
                let (_directory, program, log_file) = fake_mysql(mode);
                let option_file = log_file.with_extension("cnf");
                fs::write(&option_file, "[client]\n").unwrap();
                let result = run_mysql_single_statement_with_program(
                    OsStr::new(&program),
                    &option_file,
                    "SELECT 123",
                    AccessMode::ReadWrite,
                    Duration::from_secs(5),
                )
                .await;
                assert!(result.is_err(), "{mode}");
                let log = fs::read_to_string(format!("{}.log", option_file.display()))
                    .unwrap_or_default();
                assert!(!log.contains("SELECT 123"), "{mode}: {log}");
            }
        }

        #[tokio::test]
        async fn timeout_kills_the_process_and_discards_output() {
            let (_directory, program, log_file) = fake_mysql("timeout");
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_millis(50),
            )
            .await;

            assert!(matches!(result, Err(DbOperationError::Timeout(_))));
            let log =
                fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
            assert!(!log.contains("SELECT 123"));
        }

        #[cfg(feature = "test-support")]
        #[tokio::test]
        async fn initial_pty_output_timeout_kills_and_reaps_the_process() {
            let (_directory, program, option_file) = fake_mysql_without_output();
            let (pid, result) = run_mysql_cli_script_with_program_and_pid(
                OsStr::new(&program),
                &option_file,
                "SELECT 123;\n",
                Duration::from_millis(50),
            )
            .await
            .expect("spawn script process");

            match result {
                Err(DbOperationError::Timeout(details)) => {
                    assert!(
                        details.contains("initial MySQL PTY output wait"),
                        "{details}"
                    );
                }
                result => panic!("expected initial PTY output timeout, got {result:?}"),
            }
            let status = std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .expect("check script process");
            assert!(!status.success(), "script process {pid} is still running");
        }
    }

    mod multi_statement {
        use super::*;

        #[tokio::test]
        async fn executes_each_statement_and_returns_the_last_user_result() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let log_file = PathBuf::from(format!("{}.log", option_file.display()));
            let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT 2")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await
            .unwrap_or_else(|error| panic!("multi execution failed: {error:?}"));

            assert_eq!(
                result.result_set,
                Some(MySqlResultSet {
                    columns: vec!["value".to_string()],
                    values: vec![vec![QueryValue::Text("two".to_string())]],
                })
            );
            assert_eq!(result.command_tag, Some(CommandTag::Update(3)));
            assert_eq!(result.refresh_scope, RefreshScope::Data);
            let log = fs::read_to_string(log_file).unwrap();
            assert!(log.contains("UPDATE items SET value = 1"));
            assert!(log.matches("__sabiql_marker").count() >= 2);
        }

        #[tokio::test]
        async fn tail_error_is_classified_after_pty_drain() {
            let (_directory, program, option_file) = fake_mysql_multi_with_tail_failure();
            let query = "UPDATE items SET value = 1; CREATE TABLE created (id INT)";
            let statements = split_mysql_statements(query)
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::QueryFailedAfterChange {
                    source,
                    refresh_scope: RefreshScope::Metadata,
                    ..
                }) if matches!(&*source, DbOperationError::ObjectMissing(_))
            ));
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.lines().any(|line| line == "input_closed"), "{log}");
        }

        #[tokio::test]
        async fn marker_failure_after_a_change_refreshes_the_current_scope() {
            let (_directory, program, option_file) = fake_mysql_multi_with_marker_failure();
            let statements = split_mysql_statements("UPDATE items SET value = 1")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::QueryFailedAfterChange {
                    source,
                    refresh_scope: RefreshScope::Data,
                    ..
                }) if matches!(&*source, DbOperationError::QueryFailed(_))
            ));
        }

        #[tokio::test]
        async fn first_change_statement_failure_keeps_the_classified_error_unwrapped() {
            for (details, summary) in [
                (
                    "ERROR 1142 (42000): command denied to user",
                    "Permission denied",
                ),
                (
                    "ERROR 1062 (23000): Duplicate entry duplicate_value for key PRIMARY",
                    "Unique constraint violation",
                ),
                (
                    "ERROR 1452 (23000): Cannot add or update a child row: a foreign key constraint fails",
                    "Foreign key constraint violation",
                ),
                (
                    "ERROR 1205 (HY000): Lock wait timeout exceeded",
                    "Operation blocked by lock or timeout",
                ),
            ] {
                let (_directory, program, option_file) =
                    fake_mysql_multi_with_statement_failure(details);
                let statements = split_mysql_statements("UPDATE items SET value = 1")
                    .unwrap()
                    .into_iter()
                    .map(|sql| classify_mysql_statement(&sql).unwrap())
                    .collect::<Vec<_>>();

                let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                    OsStr::new(&program),
                    &option_file,
                    &statements,
                    AccessMode::ReadWrite,
                    None,
                    Duration::from_secs(5),
                )
                .await;
                let Err(error) = result else {
                    panic!("expected the fake MySQL statement to fail");
                };

                assert_eq!(error.summary(), summary);
                assert!(!matches!(
                    error,
                    DbOperationError::QueryFailedAfterChange { .. }
                ));
            }
        }

        #[tokio::test]
        async fn marks_a_later_failure_after_a_confirmed_change_for_refresh() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements(
                "UPDATE items SET value = 1; SELECT missing_column FROM items",
            )
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::QueryFailedAfterChange {
                    source,
                    refresh_scope: RefreshScope::Data,
                    ..
                }) if matches!(&*source, DbOperationError::ObjectMissing(_))
            ));
        }

        #[tokio::test]
        async fn rejects_error_reported_after_row_count_marker() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements("SELECT missing_column FROM items")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements_and_expected_columns(
                OsStr::new(&program),
                &option_file,
                &statements,
                AccessMode::ReadWrite,
                None,
                Duration::from_secs(5),
            )
            .await;

            assert!(matches!(
            result,
            Err(DbOperationError::ObjectMissing(details))
                if details.contains("missing_column")
            ));
        }
    }
}

#[cfg(all(test, not(unix)))]
mod windows_tests {
    use crate::adapters::csv_export::CsvFileWriter;

    use super::super::export::stream_mysql_resultset_to_csv;
    use super::*;

    #[tokio::test]
    async fn csv_stream_returns_incomplete_stderr_for_final_classification() {
        let mut child = Command::new("cmd.exe")
            .args([
                "/C",
                "echo ^<resultset^>^</resultset^> & ping -n 2 127.0.0.1 >nul & echo 054 (42S22): missing_column 1>&2",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn cmd.exe");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut process = MySqlProcess {
            child,
            stdin: Some(stdin),
            stdout,
            stderr,
            pending: Vec::new(),
            pending_stderr: b"ERROR 1".to_vec(),
            frame_scanner: MySqlResultsetFrameScanner::default(),
        };
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("export.csv");
        let mut csv_writer = CsvFileWriter::create(path).await.unwrap();

        assert!(
            stream_mysql_resultset_to_csv(&mut process, &mut csv_writer)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(process.pending_stderr, b"ERROR 1");
        csv_writer.finish().await.unwrap();

        let result =
            tokio::time::timeout(Duration::from_secs(3), finish_mysql_session(&mut process))
                .await
                .expect("finish pipe process timed out")
                .expect("finish pipe process");

        assert!(matches!(
            classify_mysql_query_failure(&result.error_bytes),
            DbOperationError::ObjectMissing(details) if details.contains("missing_column")
        ));
    }

    #[tokio::test]
    async fn pipe_finish_combines_pending_stderr_with_final_read() {
        let mut child = Command::new("cmd.exe")
            .args([
                "/C",
                "findstr /R \"^\" >nul & echo 054 (42S22): missing_column 1>&2 & exit /B 0",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn cmd.exe");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut process = MySqlProcess {
            child,
            stdin: Some(stdin),
            stdout,
            stderr,
            pending: Vec::new(),
            pending_stderr: b"ERROR 1".to_vec(),
            frame_scanner: MySqlResultsetFrameScanner::default(),
        };

        let result =
            tokio::time::timeout(Duration::from_secs(2), finish_mysql_session(&mut process))
                .await
                .expect("finish pipe process timed out waiting for stdin EOF")
                .expect("finish pipe process");

        assert!(matches!(
            classify_mysql_query_failure(&result.error_bytes),
            DbOperationError::ObjectMissing(details) if details.contains("missing_column")
        ));
    }

    #[tokio::test]
    async fn pipe_finish_shuts_down_stdin_before_draining_cli_error() {
        let mut child = Command::new("cmd.exe")
            .args([
                "/C",
                "findstr /R \"^\" >nul & echo ERROR 1054 (42S22): missing_column 1>&2 & exit /B 0",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn cmd.exe");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut process = MySqlProcess {
            child,
            stdin: Some(stdin),
            stdout,
            stderr,
            pending: Vec::new(),
            pending_stderr: Vec::new(),
            frame_scanner: MySqlResultsetFrameScanner::default(),
        };

        let result =
            tokio::time::timeout(Duration::from_secs(2), finish_mysql_session(&mut process))
                .await
                .expect("finish pipe process timed out waiting for stdin EOF")
                .expect("finish pipe process");

        assert_eq!(result.status.code(), Some(0));
        assert!(!result.forcibly_stopped);
        assert!(has_mysql_cli_error(&result.error_bytes));
        assert!(matches!(
            classify_mysql_query_failure(&result.error_bytes),
            DbOperationError::ObjectMissing(details) if details.contains("missing_column")
        ));
    }
}
