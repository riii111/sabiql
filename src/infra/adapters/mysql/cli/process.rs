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
use crate::domain::{MySqlDiagnostic, RefreshScope};

use super::args::{MYSQL_CLIENT_MAX_PACKET_BYTES, mysql_adhoc_args, mysql_query_args};
#[cfg(not(unix))]
use super::error::classify_mysql_query_failure;
use super::error::{
    classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error, validate_mode_probe,
};
#[cfg(not(unix))]
use super::pipe::{read_all, read_one_mysql_resultset_from_pipes};
use super::policy::{
    MYSQL_SESSION_MARKER_COLUMN, query_failed_after_mysql_statement, validate_mysql_session_marker,
};
#[cfg(unix)]
use super::pty::{
    MySqlPty, create_mysql_pty, read_one_pty_resultset, read_one_pty_resultset_with_diagnostics,
    read_pty_all, read_pty_until_idle,
};
use super::sanitize_mysql_command_environment;
use super::xml::{MySqlResultsetFrameScanner, parse_mysql_xml, trace_mysql_statement};

mod session;
pub(in crate::adapters::mysql) use session::MySqlMetadataSession;
mod adhoc;
pub(in crate::adapters::mysql) use adhoc::run_mysql_adhoc;
mod single;
#[cfg(feature = "test-support")]
pub(super) mod test_support;
pub(in crate::adapters::mysql) use single::run_mysql_single_statement;
mod metadata;
pub(super) use metadata::mysql_metadata_columns;

pub(in crate::adapters::mysql) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
#[cfg(unix)]
const MYSQL_PTY_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const MYSQL_SESSION_SETTINGS: &str = "SET SESSION autocommit=1, completion_type=NO_CHAIN";
const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

fn map_mysql_cli_spawn_error(error: io::Error) -> DbOperationError {
    if error.kind() == io::ErrorKind::NotFound {
        DbOperationError::CommandNotFound {
            command: DatabaseCli::MySql,
            details: error.to_string(),
        }
    } else {
        DbOperationError::ConnectionFailed(error.to_string())
    }
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
    pub(super) frame_scanner: MySqlResultsetFrameScanner,
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

    pub(in crate::adapters::mysql::cli) async fn probe_sql_mode(
        &mut self,
    ) -> Result<(), DbOperationError> {
        let probe_marker = Uuid::new_v4().simple().to_string();
        let probe_query = format!(
            "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
        );
        write_mysql_statement(self, &probe_query).await?;
        let probe_xml = read_one_mysql_resultset(self).await?;
        let probe = parse_mysql_xml(&probe_xml)?;
        validate_mode_probe(&probe, &probe_marker)
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
                frame_scanner: MySqlResultsetFrameScanner::default(),
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
                frame_scanner: MySqlResultsetFrameScanner::default(),
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
    let marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(process, MYSQL_SESSION_SETTINGS).await?;
    if access_mode.is_read_only() {
        write_mysql_statement(process, MYSQL_READ_ONLY_STATEMENT).await?;
    }
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
            Err(query_failed_after_mysql_statement(
                error,
                possible_refresh_scope,
            ))
        }
        Err(_) => {
            cleanup_mysql_process(process).await;
            Err(query_failed_after_mysql_statement(
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
) -> Result<(Vec<u8>, Vec<MySqlDiagnostic>), DbOperationError> {
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
mod process_tests {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    #[test]
    fn maps_missing_mysql_cli_to_command_not_found() {
        let error = map_mysql_cli_spawn_error(io::Error::new(
            io::ErrorKind::NotFound,
            "mysql executable was not found",
        ));

        assert!(matches!(
            error,
            DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                details,
            } if details == "mysql executable was not found"
        ));
    }

    #[test]
    fn maps_other_mysql_cli_spawn_errors_to_connection_failed() {
        let error = map_mysql_cli_spawn_error(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "mysql executable permission denied",
        ));

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailed(details)
                if details == "mysql executable permission denied"
        ));
    }
    #[test]
    fn keeps_production_query_timeout_at_31_seconds() {
        assert_eq!(MYSQL_QUERY_TIMEOUT, Duration::from_secs(31));
    }

    #[cfg(unix)]
    fn session(status: i32, forcibly_stopped: bool, error_bytes: &[u8]) -> MySqlSessionResult {
        MySqlSessionResult {
            status: ExitStatus::from_raw(status),
            forcibly_stopped,
            error_bytes: error_bytes.to_vec(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn preserves_mysql_session_exit_rules() {
        assert!(validate_mysql_session_exit(&session(0, false, b""), None).is_ok());
        assert!(matches!(
            validate_mysql_session_exit(
                &session(
                    0,
                    true,
                    b"ERROR 1054 (42S22): Unknown column missing_column"
                ),
                None,
            ),
            Err(DbOperationError::ObjectMissing(_))
        ));
        assert!(validate_mysql_session_exit(&session(1, false, b""), None).is_err());
        assert!(validate_mysql_session_exit(&session(9, true, b""), None).is_ok());
        assert!(matches!(
            validate_mysql_session_exit(
                &session(
                    1,
                    false,
                    b"ERROR 2020 (HY000): Got packet bigger than 'max_allowed_packet' bytes",
                ),
                Some(33_554_432),
            ),
            Err(DbOperationError::QueryFailed(details))
                if details == "MySQL protocol packet exceeds the 33554432-byte client limit"
        ));
    }

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

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::super::export::run_mysql_export_process;
    use super::super::policy::MySqlExecutionResult;
    use super::super::xml::MySqlResultSet;
    use super::adhoc::run_mysql_adhoc_with_program_and_statements_and_expected_columns;
    use super::metadata::{
        mysql_metadata_columns_external_with_program,
        run_mysql_metadata_query_with_read_only_session_with_timeout,
    };
    use super::single::run_mysql_single_statement_process_with_diagnostics;
    use super::single::test_support::run_mysql_single_statement_process;
    use super::*;
    use crate::adapters::csv_export::export_to_path;
    use crate::domain::mysql_sql::{classify_mysql_statement, split_mysql_statements};
    use crate::domain::{
        CommandTag, MySqlDiagnostic, MySqlDiagnosticLevel, QueryValue, RefreshScope,
    };

    mod cleanup {
        use super::*;
        use crate::adapters::mysql::dsn::parse_mysql_dsn;
        use crate::adapters::mysql::option_file::MySqlOptionFile;

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

        #[test]
        fn option_file_is_removed_when_mysql_process_start_fails() {
            let target = parse_mysql_dsn("mysql://user:secret@localhost:3306").unwrap();
            let (result, path) = {
                let option_file = MySqlOptionFile::create(&target).unwrap();
                let path = option_file.path.clone();
                let result = MySqlProcess::spawn_with_program(
                    OsStr::new("__sabiql_missing_mysql_binary__"),
                    &path,
                );
                (result, path)
            };

            assert!(result.is_err());
            assert!(!path.exists());
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
        run_mysql_process_with_timeout(
            execution_timeout,
            &mut process,
            RefreshScope::None,
            async |process| run_mysql_export_process(process, option_file, query, path).await,
        )
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
        run_mysql_process_with_timeout(
            execution_timeout,
            &mut process,
            RefreshScope::None,
            async |process| run_mysql_single_statement_process(process, query, access_mode).await,
        )
        .await
    }

    async fn run_mysql_single_statement_with_diagnostics_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        access_mode: AccessMode,
        execution_timeout: Duration,
    ) -> Result<MySqlExecutionResult, DbOperationError> {
        let mut process = MySqlProcess::spawn_with_adhoc_program(program, option_file)?;
        run_mysql_process_with_timeout(
            execution_timeout,
            &mut process,
            RefreshScope::None,
            async |process| {
                run_mysql_single_statement_process_with_diagnostics(process, query, access_mode)
                    .await
            },
        )
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
        let settings_timeout = if mode == "timeout" {
            "while :; do :; done"
        } else {
            ""
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = "$eof" ] && exit 0
  [ "$line" = ";" ] && continue
  case "$line" in
    "SET SESSION autocommit=1, completion_type=NO_CHAIN")
      {settings_timeout}
      ;;
    "SET SESSION TRANSACTION READ ONLY")
      {session_failure}
      ;;
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
      {probe_response}
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
done
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, log_file)
    }

    fn fake_mysql_single_with_warning() -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let script = r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'argv=%s\n' "$*" >> "$log"
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *"$eof"*)
      exit 0
      ;;
    ";"|"SET SESSION autocommit=1, completion_type=NO_CHAIN"|"SET SESSION TRANSACTION READ ONLY"|"SET SESSION TRANSACTION READ WRITE")
      ;;
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      ;;
    *)
      printf '%s\n' '<resultset><row><field name="value">tree</field></row></resultset>'
      printf '%s\n' 'Warning (Code 1265): truncated'
      ;;
  esac
done
"#;
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, log_file)
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
    *"SET SESSION autocommit=1, completion_type=NO_CHAIN"*)
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
        let update_response = statement_error
            .map_or_else(String::new, |error| format!("printf '%s\\n' '{error}' >&2"));
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
      case \"$line\" in
        *ROW_COUNT\\(\\)*)
          printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field><field name=\"affected_rows\">3</field></row></resultset>'
          ;;
        *)
          printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field></row></resultset>'
          ;;
      esac
      {tail_after_create}")
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
printf 'argv=%s\n' "$*" >> "$log"
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
      if printf '%s\n' "$line" | grep -q lower_case_table_names; then
        printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
      else
        printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      fi
      ;;
    "SET SESSION autocommit=1, completion_type=NO_CHAIN"|"SET SESSION TRANSACTION READ ONLY")
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
      printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2
      exit 1
      ;;
    *SLEEP*)
      while :; do :; done
      ;;
    *"INTO @"*)
      ;;
    *SELECT*)
      case "$line" in
        *WHERE\ FALSE*)
          printf '%s\n' '<resultset></resultset>'
          ;;
        *)
          value=one
          case "$line" in
            *SELECT\ 2*) value=two ;;
            *SELECT\ @picked*) value=picked ;;
          esac
          printf '%s\n' '<resultset><row><field name="value">'"$value"'</field></row></resultset>'
          ;;
      esac
      ;;
    *UPDATE*)
      last_statement=update
      {update_response}
      ;;
    *"SHOW CREATE TABLE"*)
      printf '%s\n' '<resultset><row><field name="Create Table">CREATE TABLE items (id INT)</field></row></resultset>'
      ;;
    *"INSERT IGNORE"*)
      printf '%s\n' 'Warning (Code 1062): duplicate ignored'
      ;;
    *"CREATE TABLE IF NOT EXISTS"*)
      printf '%s\n' 'Note (Code 1050): table already exists'
      last_statement=create
      ;;
    *CREATE*)
      last_statement=create
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
        async fn diagnostics_use_adhoc_args_and_follow_resultset_to_marker() {
            let (_directory, program, log_file) = fake_mysql_single_with_warning();
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_single_statement_with_diagnostics_with_program(
                OsStr::new(&program),
                &option_file,
                "EXPLAIN FORMAT=TREE SELECT 1",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            assert_eq!(
                result.result_set.unwrap().values[0][0].as_str(),
                Some("tree")
            );
            assert_eq!(
                result.diagnostics,
                vec![MySqlDiagnostic {
                    level: MySqlDiagnosticLevel::Warning,
                    code: 1265,
                    message: "truncated".to_string(),
                }]
            );
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.contains("--show-warnings"), "{log}");
            assert!(log.contains("EXPLAIN FORMAT=TREE SELECT 1"), "{log}");
            assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN), "{log}");
        }

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
            let positions = [
                MYSQL_SESSION_SETTINGS,
                MYSQL_SESSION_MARKER_COLUMN,
                "__sabiql_probe",
                "SELECT 123",
            ]
            .into_iter()
            .map(|query| log.find(query).expect("query in transcript"))
            .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
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
            let settings_index = log.find(MYSQL_SESSION_SETTINGS).expect("session settings");
            let probe_index = log.find("__sabiql_probe").expect("mode probe");
            let user_index = log.find("SELECT 2").expect("user statement");
            assert!(settings_index < session_index, "{log}");
            assert!(session_index < probe_index, "{log}");
            assert!(probe_index < user_index, "{log}");
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
            let mut session = MySqlMetadataSession::spawn_with_metadata_program(
                OsStr::new(&program),
                &option_file,
            )
            .expect("spawn fake mysql");

            session
                .prepare_read_only()
                .await
                .expect("read-only session setup");
            session.probe().await.expect("mode probe");
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
            let argv = log.lines().find(|line| line.starts_with("argv=")).unwrap();
            assert!(!argv.contains("--quick"), "{argv}");
            let positions = [
                MYSQL_SESSION_SETTINGS,
                MYSQL_READ_ONLY_STATEMENT,
                MYSQL_SESSION_MARKER_COLUMN,
                "__sabiql_probe",
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
                MYSQL_SESSION_SETTINGS,
                MYSQL_READ_ONLY_STATEMENT,
                MYSQL_SESSION_MARKER_COLUMN,
                "__sabiql_probe",
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
            assert_eq!(result.command_tag, None);
            assert_eq!(result.refresh_scope, RefreshScope::Data);
            assert!(result.diagnostics.is_empty());
            let log = fs::read_to_string(log_file).unwrap();
            assert!(log.contains("UPDATE items SET value = 1"));
            assert_eq!(log.matches("__sabiql_marker").count(), 1);
            assert!(!log.contains("ROW_COUNT()"));
        }

        #[tokio::test]
        async fn skips_resultset_wait_for_select_into_user_variable() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements =
                split_mysql_statements("SELECT id INTO @picked FROM items; SELECT @picked")
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
            .expect("SELECT INTO user variable execution");

            assert_eq!(
                result.result_set,
                Some(MySqlResultSet {
                    columns: vec!["value".to_string()],
                    values: vec![vec![QueryValue::Text("picked".to_string())]],
                })
            );
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(log.contains("SELECT id INTO @picked FROM items"), "{log}");
            assert!(log.contains("SELECT @picked"), "{log}");
        }

        #[tokio::test]
        async fn keeps_multi_statement_diagnostics_on_the_submission_result() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements(
                "INSERT IGNORE INTO items (id) VALUES (1); CREATE TABLE IF NOT EXISTS items (id INT); SELECT 2",
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
            .await
            .expect("multi-statement diagnostics execution");

            assert_eq!(
                result.result_set.as_ref().map(|result| &result.values),
                Some(&vec![vec![QueryValue::Text("two".to_string())]])
            );
            assert_eq!(
                result.diagnostics,
                vec![
                    MySqlDiagnostic {
                        level: MySqlDiagnosticLevel::Warning,
                        code: 1062,
                        message: "duplicate ignored".to_string(),
                    },
                    MySqlDiagnostic {
                        level: MySqlDiagnosticLevel::Note,
                        code: 1050,
                        message: "table already exists".to_string(),
                    },
                ]
            );
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert_eq!(log.matches("__sabiql_marker").count(), 1);
        }

        #[tokio::test]
        async fn single_dml_uses_the_submission_terminal_row_count() {
            let (_directory, program, option_file) = fake_mysql_multi();
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
            .await
            .expect("single DML execution");

            assert_eq!(result.command_tag, Some(CommandTag::Update(3)));
            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert_eq!(log.matches("__sabiql_marker").count(), 1);
            assert!(log.contains("ROW_COUNT()"));
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
        async fn first_change_statement_failure_refreshes_possible_scope() {
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
                assert!(matches!(
                    error,
                    DbOperationError::QueryFailedAfterChange {
                        source,
                        refresh_scope: RefreshScope::Data,
                    } if matches!(
                        &*source,
                        DbOperationError::PermissionDenied(_)
                            | DbOperationError::UniqueViolation(_)
                            | DbOperationError::ForeignKeyViolation(_)
                            | DbOperationError::LockTimeout(_)
                    )
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
        async fn timeout_after_a_data_change_is_wrapped_for_refresh() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT SLEEP(40)")
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
                Duration::from_millis(100),
            )
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::QueryFailedAfterChange {
                    source,
                    refresh_scope: RefreshScope::Data,
                    ..
                }) if matches!(&*source, DbOperationError::Timeout(_))
            ));
        }

        #[tokio::test]
        async fn read_only_timeout_does_not_request_refresh() {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements("SELECT SLEEP(40)")
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
                Duration::from_millis(100),
            )
            .await;

            assert!(matches!(result, Err(DbOperationError::Timeout(_))));
        }

        #[tokio::test]
        async fn rejects_error_reported_without_a_statement_marker() {
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
            client_packet_limit_bytes: None,
            preview_byte_budget: false,
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
            client_packet_limit_bytes: None,
            preview_byte_budget: false,
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
            client_packet_limit_bytes: None,
            preview_byte_budget: false,
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
