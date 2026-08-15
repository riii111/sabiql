use std::ffi::OsStr;
use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DatabaseCli, DbOperationError};
use crate::domain::QueryValue;

use super::super::args::mysql_metadata_args;
use super::super::error::{
    classify_mysql_query_failure, has_mysql_cli_error, is_mysql_batch_diagnostic,
};
use super::super::policy::{
    MYSQL_SESSION_MARKER_COLUMN, MysqlMetadataFallbackKind, mysql_metadata_select_query,
};
use super::super::probe::{run_mysql_command_with_timeout, validate_sql_mode};
use super::{MYSQL_QUERY_TIMEOUT, MysqlProcess, read_one_mysql_resultset, write_mysql_statement};

pub(in crate::adapters::mysql) async fn mysql_metadata_columns(
    process: &mut MysqlProcess,
    option_file: &std::path::Path,
    query: &str,
    kind: MysqlMetadataFallbackKind,
    access_mode: AccessMode,
) -> Result<Vec<String>, DbOperationError> {
    let query = match kind {
        MysqlMetadataFallbackKind::Select | MysqlMetadataFallbackKind::Table => {
            return mysql_metadata_select_columns(process, query).await;
        }
        MysqlMetadataFallbackKind::Show | MysqlMetadataFallbackKind::Describe => {
            query.trim().trim_end_matches(';').trim_end().to_string()
        }
    };
    mysql_metadata_columns_external(option_file, &query, access_mode).await
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
    let result = super::parse_mysql_xml(&xml)?;
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
    access_mode: AccessMode,
) -> Result<Vec<String>, DbOperationError> {
    mysql_metadata_columns_external_with_program(
        OsStr::new("mysql"),
        option_file,
        query,
        access_mode,
    )
    .await
}

pub(super) async fn mysql_metadata_columns_external_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<Vec<String>, DbOperationError> {
    if access_mode.is_read_only() {
        return run_mysql_metadata_query_with_read_only_session(program, option_file, query).await;
    }

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

struct MysqlMetadataProcess {
    child: Child,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
    stderr: BufReader<tokio::process::ChildStderr>,
}

impl MysqlMetadataProcess {
    fn spawn(program: &OsStr, option_file: &std::path::Path) -> Result<Self, DbOperationError> {
        let mut command = Command::new(program);
        command
            .args(mysql_metadata_args(option_file))
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
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        })
    }

    async fn write_statement(&mut self, query: &str) -> Result<(), DbOperationError> {
        let statement = super::mysql_statement_input(query);
        self.stdin
            .as_mut()
            .ok_or_else(|| DbOperationError::ConnectionLost("mysql stdin was closed".to_string()))?
            .write_all(&statement)
            .await
            .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))
    }

    async fn read_until_marker(&mut self, marker: &str) -> Result<Vec<u8>, DbOperationError> {
        let mut output = Vec::new();
        let mut stdout_closed = false;
        let mut stderr_closed = false;
        loop {
            if stdout_closed && stderr_closed {
                return Err(DbOperationError::QueryFailed(format!(
                    "MySQL metadata session marker was not returned: {marker}"
                )));
            }

            let mut stdout_line = Vec::new();
            let mut stderr_line = Vec::new();
            tokio::select! {
                result = self.stdout.read_until(b'\n', &mut stdout_line), if !stdout_closed => {
                    let size = result.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
                    if size == 0 {
                        stdout_closed = true;
                    } else {
                        output.extend_from_slice(&stdout_line);
                        if String::from_utf8_lossy(&stdout_line).contains(marker) {
                            return Ok(output);
                        }
                    }
                }
                result = self.stderr.read_until(b'\n', &mut stderr_line), if !stderr_closed => {
                    let size = result.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
                    if size == 0 {
                        stderr_closed = true;
                    } else if has_mysql_cli_error(&stderr_line) {
                        return Err(classify_mysql_query_failure(&stderr_line));
                    }
                }
            }
        }
    }
}

async fn finish_mysql_metadata_process(
    process: &mut MysqlMetadataProcess,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), DbOperationError> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let (stdout_result, stderr_result, status_result) = tokio::join!(
        process.stdout.read_to_end(&mut stdout),
        process.stderr.read_to_end(&mut stderr),
        process.child.wait(),
    );
    stdout_result.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    stderr_result.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let status =
        status_result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok((status, stdout, stderr))
}

async fn cleanup_mysql_metadata_process(process: &mut MysqlMetadataProcess) {
    drop(process.stdin.take());
    let _ = process.child.kill().await;
    let _ = finish_mysql_metadata_process(process).await;
}

async fn run_mysql_metadata_query_with_read_only_session(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    run_mysql_metadata_query_with_read_only_session_with_timeout(
        program,
        option_file,
        query,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn run_mysql_metadata_query_with_read_only_session_with_timeout(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    execution_timeout: Duration,
) -> Result<Vec<String>, DbOperationError> {
    let mut process = MysqlMetadataProcess::spawn(program, option_file)?;
    match timeout(
        execution_timeout,
        run_mysql_metadata_query_with_read_only_session_process(&mut process, query),
    )
    .await
    {
        Ok(Ok(columns)) => Ok(columns),
        Ok(Err(error)) => {
            cleanup_mysql_metadata_process(&mut process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_metadata_process(&mut process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

async fn run_mysql_metadata_query_with_read_only_session_process(
    process: &mut MysqlMetadataProcess,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let probe_marker = Uuid::new_v4().simple().to_string();
    let probe_query = format!(
        "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
    );
    process.write_statement(&probe_query).await?;
    let probe_output = process.read_until_marker(&probe_marker).await?;
    validate_metadata_mode_probe(&probe_output, &probe_marker)?;

    let session_marker = Uuid::new_v4().simple().to_string();
    process
        .write_statement(super::MYSQL_READ_ONLY_STATEMENT)
        .await?;
    process
        .write_statement(&format!(
            "SELECT '{session_marker}' AS {MYSQL_SESSION_MARKER_COLUMN}"
        ))
        .await?;
    let session_output = process.read_until_marker(&session_marker).await?;
    validate_metadata_session_marker(&session_output, &session_marker)?;

    process.write_statement(query).await?;
    let mut stdin = process
        .stdin
        .take()
        .ok_or_else(|| DbOperationError::ConnectionLost("mysql stdin was closed".to_string()))?;
    stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    drop(stdin);
    let (status, stdout, stderr) = finish_mysql_metadata_process(process).await?;
    if has_mysql_cli_error(&stderr) {
        return Err(classify_mysql_query_failure(&stderr));
    }
    if !status.success() {
        return Err(classify_mysql_query_failure(&stderr));
    }
    parse_mysql_metadata_header(&stdout, query)
}

fn validate_metadata_mode_probe(output: &[u8], marker: &str) -> Result<(), DbOperationError> {
    let fields = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .map(|line| line.split(|byte| *byte == b'\t').collect::<Vec<_>>())
        .find(|fields| {
            fields
                .first()
                .is_some_and(|field| *field == marker.as_bytes())
        })
        .ok_or_else(|| {
            DbOperationError::QueryFailed(
                "MySQL metadata fallback returned an invalid mode probe".to_string(),
            )
        })?;
    let sql_mode = fields.get(1).ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL metadata fallback returned an incomplete mode probe".to_string(),
        )
    })?;
    let sql_mode = String::from_utf8(sql_mode.to_vec())
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    validate_sql_mode(&sql_mode)
}

fn validate_metadata_session_marker(output: &[u8], marker: &str) -> Result<(), DbOperationError> {
    let valid = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .map(|line| line.split(|byte| *byte == b'\t').collect::<Vec<_>>())
        .any(|fields| {
            fields
                .first()
                .is_some_and(|field| *field == marker.as_bytes())
        });
    if valid {
        Ok(())
    } else {
        Err(DbOperationError::QueryFailed(
            "MySQL metadata fallback returned an invalid read-only session marker".to_string(),
        ))
    }
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
