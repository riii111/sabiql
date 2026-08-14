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
use tokio::time::timeout;
use uuid::Uuid;

use crate::app::policy::sql::mysql_statement::{MysqlStatement, MysqlStatementKind};
use crate::app::ports::outbound::{AccessMode, DatabaseCli, DbOperationError};
use crate::domain::{QueryValue, RefreshScope};

use super::args::{mysql_metadata_args, mysql_query_args};
#[cfg(unix)]
use super::error::trace_mysql_error;
use super::error::{
    classify_mysql_query_failure, has_mysql_cli_error, is_mysql_batch_diagnostic,
    validate_mode_probe,
};
#[cfg(not(unix))]
use super::pipe::{read_all, read_one_mysql_resultset_from_pipes};
use super::policy::{
    MYSQL_SESSION_MARKER_COLUMN, MysqlCommandEvent, MysqlExecutionResult,
    MysqlMetadataFallbackKind, aggregate_mysql_command_tag, is_mysql_row_count_marker,
    mysql_command_tag, mysql_metadata_fallback_has_unsupported_session_state,
    mysql_metadata_fallback_kind, mysql_metadata_select_query, mysql_refresh_scope,
    mysql_row_count_marker, query_failed_after_change, query_failed_after_mysql_statement,
    validate_mysql_session_marker,
};
use super::probe::run_mysql_command_with_timeout;
#[cfg(unix)]
use super::pty::{
    MysqlPty, create_mysql_pty, read_one_pty_resultset, read_pty_all, read_pty_until_idle,
};
#[cfg(unix)]
use super::xml::trace_mysql_frame;
use super::xml::{MysqlResultSet, MysqlResultsetFrameScanner, parse_mysql_xml};

#[cfg(all(unix, feature = "test-support"))]
use super::super::dsn::{parse_mysql_dsn, validate_mysql_tls_files, validate_mysql_values};
#[cfg(all(unix, feature = "test-support"))]
use super::super::option_file::MySqlOptionFile;

pub(in crate::adapters::mysql) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

pub(super) async fn stop_mysql_process(
    process: &mut MysqlProcess,
) -> Result<(ExitStatus, bool), DbOperationError> {
    if let Some(status) = process
        .child
        .try_wait()
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?
    {
        return Ok((status, false));
    }
    // Callers drain the PTY first because mysql --binary-mode does not accept quit commands.
    let _ = process.child.kill().await;
    let status = process
        .child
        .wait()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, true))
}

#[cfg(not(unix))]
async fn finish_mysql_pipe_process(
    process: &mut MysqlProcess,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), DbOperationError> {
    let (stdout, stderr, status) = tokio::join!(
        read_all(&mut process.stdout),
        read_all(&mut process.stderr),
        process.child.wait()
    );
    let stdout = stdout.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let status = status.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, stdout, stderr))
}

pub(in crate::adapters::mysql) struct MysqlProcess {
    pub(super) child: Child,
    #[cfg(unix)]
    pub(super) pty: MysqlPty,
    #[cfg(not(unix))]
    pub(super) stdin: ChildStdin,
    #[cfg(not(unix))]
    pub(super) stdout: ChildStdout,
    #[cfg(not(unix))]
    pub(super) stderr: ChildStderr,
    #[cfg(not(unix))]
    pub(super) pending: Vec<u8>,
    #[cfg(not(unix))]
    pub(super) pending_stderr: Vec<u8>,
    #[cfg(not(unix))]
    pub(super) frame_scanner: MysqlResultsetFrameScanner,
}

impl MysqlProcess {
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
                stdin,
                stdout,
                stderr,
                pending: Vec::new(),
                pending_stderr: Vec::new(),
                frame_scanner: MysqlResultsetFrameScanner::default(),
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
            pty: MysqlPty {
                input,
                output,
                pending: Vec::new(),
                frame_scanner: MysqlResultsetFrameScanner::default(),
            },
        })
    }
}

pub(in crate::adapters::mysql) struct MysqlMetadataSession {
    process: MysqlProcess,
}

impl MysqlMetadataSession {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Ok(Self {
            process: MysqlProcess::spawn_with_program(program, option_file)?,
        })
    }

    pub(in crate::adapters::mysql) async fn probe(&mut self) -> Result<(), DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let query =
            format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
        let result = self.execute(&query).await?;
        validate_mode_probe(&result, &marker)
    }

    pub(in crate::adapters::mysql) async fn execute(
        &mut self,
        query: &str,
    ) -> Result<MysqlResultSet, DbOperationError> {
        write_mysql_statement(&mut self.process, query).await?;
        let xml = read_one_mysql_resultset(&mut self.process).await?;
        parse_mysql_xml(&xml)
    }

    pub(in crate::adapters::mysql) async fn finish(&mut self) -> Result<(), DbOperationError> {
        #[cfg(not(unix))]
        self.process
            .stdin
            .shutdown()
            .await
            .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

        #[cfg(unix)]
        let tail = {
            write_mysql_input(&mut self.process, b"\x04").await?;
            read_pty_until_idle(&mut self.process.pty)
                .await
                .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
        };

        #[cfg(not(unix))]
        let (status, _stdout, stderr) = finish_mysql_pipe_process(&mut self.process).await?;
        #[cfg(unix)]
        let (status, forcibly_stopped) = stop_mysql_process(&mut self.process).await?;
        #[cfg(not(unix))]
        let forcibly_stopped = false;
        #[cfg(unix)]
        let error_bytes = tail.as_slice();
        #[cfg(not(unix))]
        let error_bytes = stderr.as_slice();
        if has_mysql_cli_error(error_bytes) {
            return Err(classify_mysql_query_failure(error_bytes));
        }
        if !status.success() && !forcibly_stopped {
            return Err(classify_mysql_query_failure(error_bytes));
        }
        Ok(())
    }

    pub(in crate::adapters::mysql) async fn cleanup(&mut self) {
        cleanup_mysql_process(&mut self.process).await;
    }
}

pub(in crate::adapters::mysql) async fn run_mysql_adhoc(
    option_file: &std::path::Path,
    query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    run_mysql_adhoc_with_program_and_statements(
        OsStr::new("mysql"),
        option_file,
        query,
        statements,
        access_mode,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(in crate::adapters::mysql) async fn run_mysql_single_statement(
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), option_file)?;
    let result = timeout(
        MYSQL_QUERY_TIMEOUT,
        run_mysql_single_statement_process(&mut process, query, access_mode),
    )
    .await;
    match result {
        Ok(Ok(result_set)) => Ok(result_set),
        Ok(Err(error)) => {
            cleanup_mysql_process(&mut process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_process(&mut process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

async fn run_mysql_single_statement_process(
    process: &mut MysqlProcess,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, access_mode).await?;

    write_mysql_statement(process, query).await?;

    #[cfg(not(unix))]
    process
        .stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

    #[cfg(unix)]
    let (stdout, tail) = {
        let stdout = read_one_mysql_resultset(process).await?;
        write_mysql_input(process, b"\x04").await?;
        let tail = read_pty_until_idle(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        (stdout, tail)
    };

    #[cfg(not(unix))]
    let (status, stdout, stderr) = finish_mysql_pipe_process(process).await?;
    #[cfg(unix)]
    let (status, forcibly_stopped) = stop_mysql_process(process).await?;
    #[cfg(not(unix))]
    let forcibly_stopped = false;
    #[cfg(unix)]
    let error_bytes = tail.as_slice();
    #[cfg(not(unix))]
    let error_bytes = stderr.as_slice();
    if has_mysql_cli_error(error_bytes) {
        return Err(classify_mysql_query_failure(error_bytes));
    }
    if !status.success() && !forcibly_stopped {
        return Err(classify_mysql_query_failure(error_bytes));
    }
    parse_mysql_xml(&stdout)
}

async fn run_mysql_adhoc_with_program_and_statements(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<MysqlExecutionResult, DbOperationError> {
    if mysql_metadata_fallback_has_unsupported_session_state(statements) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL empty SHOW/DESCRIBE metadata fallback cannot preserve temporary-table session state"
                .to_string(),
        ));
    }
    let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
    let result = timeout(
        execution_timeout,
        run_mysql_adhoc_process(&mut process, option_file, query, statements, access_mode),
    )
    .await;

    match result {
        Ok(Ok(result_set)) => Ok(result_set),
        Ok(Err(error)) => {
            cleanup_mysql_process(&mut process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_process(&mut process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

async fn run_mysql_adhoc_process(
    process: &mut MysqlProcess,
    option_file: &std::path::Path,
    _query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    let probe_marker = Uuid::new_v4().simple().to_string();
    let probe_query = format!(
        "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
    );
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &probe_marker)?;
    configure_mysql_session(process, access_mode).await?;

    let mut last_result_set = None;
    let mut command_tags = Vec::with_capacity(statements.len());
    let mut refresh_scope = RefreshScope::None;

    for statement in statements {
        let marker = Uuid::new_v4().simple().to_string();
        let statement_scope = mysql_refresh_scope(&statement.kind);
        let possible_refresh_scope = refresh_scope.merge(statement_scope);
        if let Err(error) = write_mysql_statement(process, &statement.sql).await {
            return Err(query_failed_after_change(error, refresh_scope));
        }
        let marker_query =
            format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows");
        if let Err(error) = write_mysql_statement(process, &marker_query).await {
            return Err(query_failed_after_change(error, possible_refresh_scope));
        }
        let first_xml = match read_one_mysql_resultset(process).await {
            Ok(xml) => xml,
            Err(error) => {
                return Err(query_failed_after_mysql_statement(
                    error,
                    refresh_scope,
                    possible_refresh_scope,
                ));
            }
        };
        let first_result = match parse_mysql_xml(&first_xml) {
            Ok(result) => result,
            Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
        };
        let (user_result, marker_result) = if is_mysql_row_count_marker(&first_result, &marker) {
            (None, first_result)
        } else {
            let xml = match read_one_mysql_resultset(process).await {
                Ok(xml) => xml,
                Err(error) => {
                    return Err(query_failed_after_mysql_statement(
                        error,
                        refresh_scope,
                        possible_refresh_scope,
                    ));
                }
            };
            let marker_result = match parse_mysql_xml(&xml) {
                Ok(result) => result,
                Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
            };
            let user_result = fill_mysql_empty_result_columns(
                process,
                first_result,
                option_file,
                &statement.sql,
                &statement.kind,
            )
            .await
            .map_err(|error| query_failed_after_change(error, possible_refresh_scope))?;
            (Some(user_result), marker_result)
        };
        let affected_rows = match mysql_row_count_marker(&marker_result, &marker) {
            Ok(rows) => rows,
            Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
        };
        if let Some(result) = user_result {
            last_result_set = Some(result);
        }
        let tag = mysql_command_tag(&statement.kind, affected_rows, last_result_set.as_ref());
        command_tags.push(MysqlCommandEvent {
            kind: statement.kind.clone(),
            target: statement.target.clone(),
            tag,
        });
        refresh_scope = possible_refresh_scope;
    }

    #[cfg(not(unix))]
    process
        .stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

    #[cfg(unix)]
    let tail = {
        write_mysql_input(process, b"\x04").await?;
        let tail = read_pty_until_idle(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        trace_mysql_frame("discard tail", tail.len());
        trace_mysql_error(&tail);
        tail
    };

    #[cfg(not(unix))]
    let (status, _stdout, stderr) = finish_mysql_pipe_process(process).await?;
    #[cfg(unix)]
    let (status, forcibly_stopped) = stop_mysql_process(process).await?;
    #[cfg(not(unix))]
    let forcibly_stopped = false;

    #[cfg(unix)]
    let error_bytes = tail.as_slice();
    #[cfg(not(unix))]
    let error_bytes = stderr.as_slice();
    if has_mysql_cli_error(error_bytes) {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(error_bytes),
            refresh_scope,
        ));
    }
    if !status.success() && !forcibly_stopped {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(error_bytes),
            refresh_scope,
        ));
    }

    Ok(MysqlExecutionResult {
        result_set: last_result_set,
        command_tag: aggregate_mysql_command_tag(&command_tags),
        refresh_scope,
    })
}

pub(super) async fn configure_mysql_session(
    process: &mut MysqlProcess,
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
    process: &mut MysqlProcess,
    query: &str,
) -> Result<(), DbOperationError> {
    let query = query.trim_end();
    write_mysql_input(process, query.as_bytes()).await?;
    if query.ends_with(';') {
        write_mysql_input(process, b"\n").await
    } else if mysql_statement_has_trailing_line_comment(query) {
        write_mysql_input(process, b"\n;\n").await
    } else {
        write_mysql_input(process, b";\n").await
    }
}

fn mysql_statement_has_trailing_line_comment(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut quote = None;
    let mut line_comment = false;
    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' && delimiter != b'`' {
                index += 2;
            } else if bytes[index] == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                } else {
                    quote = None;
                    index += 1;
                }
            } else {
                index += 1;
            }
            continue;
        }
        if line_comment {
            if bytes[index] == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            quote = Some(bytes[index]);
            index += 1;
        } else if mysql_is_line_comment_start(bytes, index) {
            let comment_start = index;
            index = mysql_skip_line_comment(bytes, index);
            line_comment = !bytes[comment_start..index].contains(&b'\n');
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index = mysql_skip_block_comment(bytes, index);
        } else {
            index += 1;
        }
    }
    line_comment
}

fn mysql_is_line_comment_start(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b'#'
        || (bytes.get(index..index + 2) == Some(b"--")
            && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
}

fn mysql_skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'\n' {
            break;
        }
    }
    index
}

fn mysql_skip_block_comment(bytes: &[u8], index: usize) -> usize {
    let mut cursor = index + 2;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return cursor + 2;
        }
        cursor += 1;
    }
    bytes.len()
}

pub(super) async fn write_mysql_input(
    process: &mut MysqlProcess,
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
        .flush()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok(())
}

pub(super) async fn cleanup_mysql_process(process: &mut MysqlProcess) {
    let _ = process.child.kill().await;
    #[cfg(unix)]
    let _ = read_pty_all(&mut process.pty).await;
    #[cfg(not(unix))]
    let _ = tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
    let _ = process.child.wait().await;
}

pub(super) async fn read_one_mysql_resultset(
    process: &mut MysqlProcess,
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

pub(super) async fn mysql_metadata_columns(
    process: &mut MysqlProcess,
    option_file: &std::path::Path,
    query: &str,
    kind: MysqlMetadataFallbackKind,
) -> Result<Vec<String>, DbOperationError> {
    let query = match kind {
        MysqlMetadataFallbackKind::Select | MysqlMetadataFallbackKind::Table => {
            return mysql_metadata_select_columns(process, query).await;
        }
        MysqlMetadataFallbackKind::Show | MysqlMetadataFallbackKind::Describe => {
            query.trim().trim_end_matches(';').trim_end().to_string()
        }
    };
    mysql_metadata_columns_external(option_file, &query).await
}

async fn mysql_metadata_select_columns(
    process: &mut MysqlProcess,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let suffix = Uuid::new_v4().simple().to_string();
    let source_alias = format!("__sabiql_metadata_source_{suffix}");
    let marker_alias = format!("__sabiql_metadata_marker_{suffix}");
    let query = mysql_metadata_select_query(query, &source_alias, &marker_alias)?;
    write_mysql_statement(process, &query).await?;
    let xml = match read_one_mysql_resultset(process).await {
        Err(DbOperationError::QueryFailed(details))
            if details
                .to_ascii_lowercase()
                .contains("duplicate column name") =>
        {
            return Err(DbOperationError::UnsupportedOperation(
                "MySQL SELECT metadata fallback does not support duplicate column names"
                    .to_string(),
            ));
        }
        result => result?,
    };
    let result = parse_mysql_xml(&xml)?;
    let row = result.values.first().ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL SELECT metadata fallback returned no synthetic row".to_string(),
        )
    })?;
    if result.values.len() != 1
        || result.columns.is_empty()
        || row.len() != result.columns.len()
        || row.iter().any(|value| !matches!(value, QueryValue::Null))
    {
        return Err(DbOperationError::QueryFailed(
            "MySQL SELECT metadata fallback returned an invalid synthetic row".to_string(),
        ));
    }
    Ok(result.columns)
}

async fn mysql_metadata_columns_external(
    option_file: &std::path::Path,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let mut args = mysql_metadata_args(option_file);
    args.push(format!("--execute={query}"));
    let option_file = option_file.to_path_buf();
    let output = run_mysql_command_with_timeout(
        args,
        Some(&option_file),
        MYSQL_QUERY_TIMEOUT,
        "mysql query exceeded the execution timeout",
    )
    .await?;
    if !output.status.success() {
        return Err(classify_mysql_query_failure(&output.stderr));
    }
    parse_mysql_metadata_header(&output.stdout, query)
}

fn parse_mysql_metadata_header(
    output: &[u8],
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let query = query.trim_end();
    let query_with_semicolon = format!("{query};");
    let lines = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| {
            !line.is_empty()
                && !is_mysql_batch_diagnostic(line)
                && *line != query.as_bytes()
                && *line != query_with_semicolon.as_bytes()
        })
        .collect::<Vec<_>>();
    let header = lines.first().ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL metadata fallback returned no column header".to_string(),
        )
    })?;
    let columns = header
        .split(|byte| *byte == b'\t')
        .map(|column| {
            String::from_utf8(column.to_vec()).map_err(|error| {
                DbOperationError::QueryFailed(format!(
                    "invalid MySQL metadata fallback column name: {error}"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if columns.is_empty() || columns.iter().any(String::is_empty) {
        return Err(DbOperationError::QueryFailed(
            "MySQL metadata fallback returned an invalid column header".to_string(),
        ));
    }
    if lines.len() != 1 {
        return Err(DbOperationError::QueryFailed(
            "MySQL metadata fallback returned data instead of a header".to_string(),
        ));
    }
    Ok(columns)
}

async fn fill_mysql_empty_result_columns(
    process: &mut MysqlProcess,
    mut result: MysqlResultSet,
    option_file: &std::path::Path,
    query: &str,
    kind: &MysqlStatementKind,
) -> Result<MysqlResultSet, DbOperationError> {
    if !result.columns.is_empty() || !result.values.is_empty() {
        return Ok(result);
    }
    let fallback_kind = mysql_metadata_fallback_kind(kind).ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL empty result has no supported metadata fallback".to_string(),
        )
    })?;
    result.columns = mysql_metadata_columns(process, option_file, query, fallback_kind).await?;
    Ok(result)
}

#[cfg(all(unix, feature = "test-support"))]
pub(in crate::adapters::mysql) async fn run_mysql_cli_script_for_test(
    dsn: &str,
    script: &str,
) -> Result<Vec<u8>, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    validate_mysql_tls_files(&target)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    let result = async {
        write_mysql_input(&mut process, script.as_bytes()).await?;
        write_mysql_input(&mut process, b"\x04").await?;
        read_pty_until_idle(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
    }
    .await;
    if result.is_err() {
        cleanup_mysql_process(&mut process).await;
    } else {
        let _ = stop_mysql_process(&mut process).await;
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

    use crate::adapters::csv_export::export_to_path;
    use crate::app::policy::sql::mysql_statement::{
        classify_mysql_statement, split_mysql_statements,
    };
    use crate::domain::CommandTag;

    use super::super::export::run_mysql_export_process;
    use super::*;

    async fn export_mysql_csv_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        path: PathBuf,
        execution_timeout: Duration,
    ) -> Result<(), DbOperationError> {
        let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
        let result = timeout(
            execution_timeout,
            run_mysql_export_process(&mut process, option_file, query, path),
        )
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                cleanup_mysql_process(&mut process).await;
                Err(error)
            }
            Err(_) => {
                cleanup_mysql_process(&mut process).await;
                Err(DbOperationError::Timeout(
                    "mysql query exceeded the execution timeout".to_string(),
                ))
            }
        }
    }
    async fn run_mysql_adhoc_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        access_mode: AccessMode,
        execution_timeout: Duration,
    ) -> Result<MysqlResultSet, DbOperationError> {
        let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
        let result = timeout(
            execution_timeout,
            run_mysql_single_statement_process(&mut process, query, access_mode),
        )
        .await;
        match result {
            Ok(Ok(result_set)) => Ok(result_set),
            Ok(Err(error)) => {
                cleanup_mysql_process(&mut process).await;
                Err(error)
            }
            Err(_) => {
                cleanup_mysql_process(&mut process).await;
                Err(DbOperationError::Timeout(
                    "mysql query exceeded the execution timeout".to_string(),
                ))
            }
        }
    }

    fn fake_mysql(mode: &str) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let program = directory.path().join("mysql");
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
      "SET SESSION TRANSACTION READ ONLY;")
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
            "printf '%s\\n' 'ERROR 1054 (42S02): tail error' >&2\n  exit 1"
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
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      ;;
      *__sabiql_marker*)
      {marker_response}
      ;;
    *missing_column*)
      pending_error=1
      ;;
    *SELECT*)
      value=one
      case "$line" in *SELECT\ 2*) value=two ;; esac
      printf '%s\n' '<resultset><row><field name="value">'"$value"'</field></row></resultset>'
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

    #[tokio::test]
    async fn sends_user_sql_only_after_a_valid_mode_probe() {
        let (_directory, program, log_file) = fake_mysql("success");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
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
    async fn configures_read_only_session_before_user_sql() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let statements = split_mysql_statements("SELECT 2")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "SELECT 2",
            &statements,
            AccessMode::ReadOnly,
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
    async fn metadata_session_reuses_one_process_for_ordered_resultsets() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let mut session =
            MysqlMetadataSession::spawn_with_program(OsStr::new(&program), &option_file)
                .expect("spawn fake mysql");

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
    async fn read_only_session_failure_never_writes_user_sql() {
        let (_directory, program, log_file) = fake_mysql("read_only_failure");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
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
    async fn generated_preview_and_metadata_queries_skip_read_only_session_setup() {
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

            run_mysql_adhoc_with_program_and_statements(
                OsStr::new(&program),
                &option_file,
                query,
                &statements,
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(!log.contains(MYSQL_READ_ONLY_STATEMENT), "{query}: {log}");
        }
    }

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
    async fn export_configures_read_only_session_before_user_sql() {
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
    async fn export_ignores_cli_error_text_inside_resultset_fields() {
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
    async fn export_read_only_session_failure_never_writes_user_sql_or_partial_file() {
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
    async fn export_failure_removes_the_partial_file() {
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
    async fn export_timeout_kills_the_process_and_removes_the_partial_file() {
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

    #[tokio::test]
    async fn probe_failure_never_writes_user_sql() {
        for mode in ["unsupported", "invalid", "missing"] {
            let (_directory, program, log_file) = fake_mysql(mode);
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_adhoc_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;
            assert!(result.is_err(), "{mode}");
            let log =
                fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
            assert!(!log.contains("SELECT 123"), "{mode}: {log}");
        }
    }

    #[tokio::test]
    async fn probe_timeout_kills_the_process_and_discards_output() {
        let (_directory, program, log_file) = fake_mysql("timeout");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadWrite,
            Duration::from_millis(50),
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::Timeout(_))));
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
        assert!(!log.contains("SELECT 123"));
    }

    #[tokio::test]
    async fn nonzero_cli_exit_discards_any_collected_stdout() {
        let (_directory, program, log_file) = fake_mysql("failure");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
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
        let result = run_mysql_adhoc_with_program(
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
    async fn executes_each_statement_and_returns_the_last_user_result() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT 2")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1; SELECT 2",
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_else(|error| panic!("multi execution failed: {error:?}"));

        assert_eq!(
            result.result_set,
            Some(MysqlResultSet {
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
    async fn tail_error_preserves_the_cumulative_refresh_scope() {
        let (_directory, program, option_file) = fake_mysql_multi_with_tail_failure();
        let query = "UPDATE items SET value = 1; CREATE TABLE created (id INT)";
        let statements = split_mysql_statements(query)
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            query,
            &statements,
            AccessMode::ReadWrite,
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
    }

    #[tokio::test]
    async fn marker_failure_after_a_change_refreshes_the_current_scope() {
        let (_directory, program, option_file) = fake_mysql_multi_with_marker_failure();
        let statements = split_mysql_statements("UPDATE items SET value = 1")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1",
            &statements,
            AccessMode::ReadWrite,
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

            let result = run_mysql_adhoc_with_program_and_statements(
                OsStr::new(&program),
                &option_file,
                "UPDATE items SET value = 1",
                &statements,
                AccessMode::ReadWrite,
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
        let statements =
            split_mysql_statements("UPDATE items SET value = 1; SELECT missing_column FROM items")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1; SELECT missing_column FROM items",
            &statements,
            AccessMode::ReadWrite,
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

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "SELECT missing_column FROM items",
            &statements,
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
}
