use std::ffi::OsStr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{DatabaseDiagnostic, QueryValue};

use super::super::args::mysql_metadata_args;
use super::super::error::{
    classify_mysql_query_failure, has_mysql_cli_error, is_mysql_batch_diagnostic,
    map_mysql_cli_spawn_error,
};
use super::super::policy::{MySqlMetadataFallbackKind, mysql_metadata_select_query};
use super::super::probe::{run_mysql_command_with_timeout, validate_sql_mode};
use super::super::sanitize_mysql_command_environment;
use super::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, finish_mysql_pipe,
    read_one_mysql_resultset_with_diagnostics, write_mysql_statement,
};

pub(in crate::adapters::mysql::cli) async fn mysql_metadata_columns_with_diagnostics(
    process: &mut MySqlProcess,
    option_file: &std::path::Path,
    query: &str,
    kind: MySqlMetadataFallbackKind,
    access_mode: AccessMode,
) -> Result<(Vec<String>, Vec<DatabaseDiagnostic>), DbOperationError> {
    let query = match kind {
        MySqlMetadataFallbackKind::Select | MySqlMetadataFallbackKind::Table => {
            return mysql_metadata_select_columns_with_diagnostics(process, query).await;
        }
        MySqlMetadataFallbackKind::Show | MySqlMetadataFallbackKind::Describe => {
            query.trim().trim_end_matches(';').trim_end().to_string()
        }
    };
    Ok((
        mysql_metadata_columns_external_with_program(
            OsStr::new("mysql"),
            option_file,
            &query,
            access_mode,
        )
        .await?,
        Vec::new(),
    ))
}

async fn mysql_metadata_select_columns_with_diagnostics(
    process: &mut MySqlProcess,
    query: &str,
) -> Result<(Vec<String>, Vec<DatabaseDiagnostic>), DbOperationError> {
    let suffix = Uuid::new_v4().simple().to_string();
    let source_alias = format!("__sabiql_metadata_source_{suffix}");
    let marker_alias = format!("__sabiql_metadata_marker_{suffix}");
    let query = mysql_metadata_select_query(query, &source_alias, &marker_alias)?;
    write_mysql_statement(process, &query).await?;
    let (xml, diagnostics) = match read_one_mysql_resultset_with_diagnostics(process).await {
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
    Ok((result.columns, diagnostics))
}

pub(super) async fn mysql_metadata_columns_external_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<Vec<String>, DbOperationError> {
    if access_mode.is_read_only() {
        return run_mysql_metadata_query_with_read_only_session_with_timeout(
            program,
            option_file,
            query,
            MYSQL_QUERY_TIMEOUT,
        )
        .await;
    }

    let mut args = mysql_metadata_args(option_file);
    args.push(format!("--execute={query}"));
    let output = run_mysql_command_with_timeout(
        args,
        MYSQL_QUERY_TIMEOUT,
        "mysql query exceeded the execution timeout",
    )
    .await?;
    if !output.status.success() {
        return Err(classify_mysql_query_failure(&output.stderr));
    }
    parse_mysql_metadata_header(&output.stdout, query)
}

struct MySqlMetadataProcess {
    child: Child,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
    stderr: BufReader<tokio::process::ChildStderr>,
}

impl MySqlMetadataProcess {
    fn spawn(program: &OsStr, option_file: &std::path::Path) -> Result<Self, DbOperationError> {
        let mut command = Command::new(program);
        command
            .args(mysql_metadata_args(option_file))
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

async fn cleanup_mysql_metadata_process(process: &mut MySqlMetadataProcess) {
    drop(process.stdin.take());
    let _ = process.child.kill().await;
    let _ = finish_mysql_pipe(&mut process.stdout, &mut process.stderr, &mut process.child).await;
}

pub(super) async fn run_mysql_metadata_query_with_read_only_session_with_timeout(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    execution_timeout: Duration,
) -> Result<Vec<String>, DbOperationError> {
    let mut process = MySqlMetadataProcess::spawn(program, option_file)?;
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
    process: &mut MySqlMetadataProcess,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let session_marker = Uuid::new_v4().simple().to_string();
    process
        .write_statement(super::MYSQL_SESSION_SETTINGS)
        .await?;
    process
        .write_statement(super::MYSQL_READ_ONLY_STATEMENT)
        .await?;
    process
        .write_statement(&super::mysql_session_probe_query(&session_marker))
        .await?;
    let session_output = process.read_until_marker(&session_marker).await?;
    validate_metadata_session_probe(&session_output, &session_marker)?;

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
    let (status, stdout, stderr) =
        finish_mysql_pipe(&mut process.stdout, &mut process.stderr, &mut process.child).await?;
    if has_mysql_cli_error(&stderr) {
        return Err(classify_mysql_query_failure(&stderr));
    }
    if !status.success() {
        return Err(classify_mysql_query_failure(&stderr));
    }
    parse_mysql_metadata_header(&stdout, query)
}

fn validate_metadata_session_probe(output: &[u8], marker: &str) -> Result<(), DbOperationError> {
    let fields = parse_metadata_marker_fields(output, marker).ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL metadata fallback returned an invalid read-only session marker".to_string(),
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

fn parse_metadata_marker_fields<'a>(output: &'a [u8], marker: &str) -> Option<Vec<&'a [u8]>> {
    output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .map(|line| line.split(|byte| *byte == b'\t').collect::<Vec<_>>())
        .find(|fields| {
            fields
                .first()
                .is_some_and(|field| *field == marker.as_bytes())
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_crlf_marker_rows_and_ignores_substrings() {
        let fields = parse_metadata_marker_fields(
            b"\nmarker-prefix\twrong\r\nmarker\tSTRICT_TRANS_TABLES\r\n",
            "marker",
        )
        .expect("marker row");

        assert_eq!(
            fields,
            vec![b"marker".as_slice(), b"STRICT_TRANS_TABLES".as_slice()]
        );
    }

    #[test]
    fn preserves_marker_and_header_validation_boundaries() {
        assert!(
            validate_metadata_session_probe(b"\r\nmarker\tSTRICT_TRANS_TABLES\r\n", "marker")
                .is_ok()
        );
        assert!(matches!(
            validate_metadata_session_probe(b"marker-prefix\n", "marker"),
            Err(DbOperationError::QueryFailed(details))
                if details == "MySQL metadata fallback returned an invalid read-only session marker"
        ));
        assert!(matches!(
            validate_metadata_session_probe(b"marker\n", "marker"),
            Err(DbOperationError::QueryFailed(details))
                if details == "MySQL metadata fallback returned an incomplete mode probe"
        ));
        assert!(validate_metadata_session_probe(b"marker\t\xff\n", "marker").is_err());
        assert!(
            validate_metadata_session_probe(
                b"marker\tSTRICT_TRANS_TABLES,ANSI_QUOTES\n",
                "marker",
            )
                .is_err()
        );
        assert_eq!(
            parse_mysql_metadata_header(b"SHOW DATABASES;\r\nDatabase\r\n", "SHOW DATABASES")
                .unwrap(),
            vec!["Database".to_string()]
        );
    }
}
