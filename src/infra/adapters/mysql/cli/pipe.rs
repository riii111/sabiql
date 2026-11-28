use tokio::io::{AsyncRead, AsyncReadExt};

use crate::app::ports::outbound::DbOperationError;

use super::error::{classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error};
use super::process::read_all_bytes;
use super::xml::{
    MySqlResultSetFrameScanner,
    take_mysql_resultset_frame_after_error_check_with_diagnostics_and_preview_budget,
    trace_mysql_frame,
};

pub(super) async fn read_one_mysql_resultset_from_pipes<R, E>(
    reader: &mut R,
    stderr: &mut E,
    child: &mut tokio::process::Child,
    pending: &mut Vec<u8>,
    pending_stderr: &mut Vec<u8>,
    frame_scanner: &mut MySqlResultSetFrameScanner,
    client_packet_limit_bytes: Option<usize>,
    preview_byte_budget: bool,
) -> Result<Vec<u8>, DbOperationError>
where
    R: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    Ok(read_one_mysql_resultset_from_pipes_with_diagnostics(
        reader,
        stderr,
        child,
        pending,
        pending_stderr,
        frame_scanner,
        client_packet_limit_bytes,
        preview_byte_budget,
    )
    .await?
    .0)
}

pub(super) async fn read_one_mysql_resultset_from_pipes_with_diagnostics<R, E>(
    reader: &mut R,
    stderr: &mut E,
    child: &mut tokio::process::Child,
    pending: &mut Vec<u8>,
    pending_stderr: &mut Vec<u8>,
    frame_scanner: &mut MySqlResultSetFrameScanner,
    client_packet_limit_bytes: Option<usize>,
    preview_byte_budget: bool,
) -> Result<(Vec<u8>, Vec<crate::domain::DatabaseDiagnostic>), DbOperationError>
where
    R: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    let mut chunk = [0; 4096];
    let mut stderr_chunk = [0; 4096];
    let mut stderr_closed = false;
    loop {
        if frame_scanner.frame_bounds(pending).is_some() && !stderr_closed {
            tokio::select! {
                biased;
                result = stderr.read(&mut stderr_chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        stderr_closed = true;
                    } else {
                        pending_stderr.extend_from_slice(&stderr_chunk[..count]);
                    }
                }
                () = tokio::task::yield_now() => {}
            }
        }
        if let Some(frame) =
            take_mysql_resultset_frame_after_error_check_with_diagnostics_and_preview_budget(
                pending,
                pending_stderr,
                frame_scanner,
                client_packet_limit_bytes,
                preview_byte_budget,
            )?
        {
            trace_mysql_frame("receive resultset", frame.0.len());
            return Ok(frame);
        }
        let stdout_count = tokio::select! {
            result = reader.read(&mut chunk) => Some(
                result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?,
            ),
            result = stderr.read(&mut stderr_chunk), if !stderr_closed => {
                let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                if count == 0 {
                    stderr_closed = true;
                } else {
                    pending_stderr.extend_from_slice(&stderr_chunk[..count]);
                }
                None
            }
        };
        if let Some(count) = stdout_count {
            if count == 0 {
                finish_mysql_pipe_after_stdout_eof(stderr, child, pending_stderr).await?;
                return Err(mysql_pipe_empty_response_or_error(
                    pending_stderr,
                    client_packet_limit_bytes,
                ));
            }
            pending.extend_from_slice(&chunk[..count]);
        }
    }
}

async fn finish_mysql_pipe_after_stdout_eof<E>(
    stderr: &mut E,
    child: &mut tokio::process::Child,
    pending_stderr: &mut Vec<u8>,
) -> Result<(), DbOperationError>
where
    E: AsyncRead + Unpin,
{
    let (stderr, status) = tokio::join!(read_all_bytes(stderr), child.wait());
    let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    status.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    pending_stderr.extend_from_slice(&stderr);
    Ok(())
}

fn mysql_pipe_empty_response_or_error(
    pending_stderr: &[u8],
    client_packet_limit_bytes: Option<usize>,
) -> DbOperationError {
    if has_mysql_cli_error(pending_stderr) {
        classify_mysql_query_failure_with_packet_limit(pending_stderr, client_packet_limit_bytes)
    } else {
        DbOperationError::QueryFailed("mysql mode probe returned no resultset".to_string())
    }
}

#[cfg(test)]
#[cfg(not(unix))]
mod tests {
    use std::process::Stdio;

    use tokio::io::AsyncWriteExt;
    use tokio::process::{Child, Command};

    use super::*;

    fn exited_child() -> Child {
        Command::new("cmd.exe")
            .args(["/C", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cmd.exe")
    }

    #[tokio::test]
    async fn pipe_errors_are_checked_before_resultset_frames() {
        let (mut stdout_writer, mut stdout_reader) = tokio::io::duplex(1024);
        let (mut stderr_writer, mut stderr_reader) = tokio::io::duplex(1024);
        stdout_writer
            .write_all(b"<resultset><row></row></resultset>")
            .await
            .unwrap();
        stderr_writer
            .write_all(b"ERROR 1054 (42S22): Unknown column missing_column\n")
            .await
            .unwrap();
        drop(stdout_writer);
        drop(stderr_writer);
        let mut frame_scanner = MySqlResultSetFrameScanner::default();
        let mut child = exited_child();

        let result = read_one_mysql_resultset_from_pipes(
            &mut stdout_reader,
            &mut stderr_reader,
            &mut child,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut frame_scanner,
            None,
            false,
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    }

    #[tokio::test]
    async fn drains_delayed_stderr_after_stdout_eof_before_classifying() {
        let mut child = Command::new("pwsh.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$stream = [Console]::OpenStandardOutput(); $stream.Write([Text.Encoding]::UTF8.GetBytes('result')); $stream.Close(); Start-Sleep -Milliseconds 100; [Console]::Error.WriteLine('ERROR 1054 (42S22): Unknown column missing_column')",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn pwsh.exe");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let mut pending = Vec::new();
        let mut pending_stderr = Vec::new();
        let mut frame_scanner = MySqlResultSetFrameScanner::default();

        let result = read_one_mysql_resultset_from_pipes(
            &mut stdout,
            &mut stderr,
            &mut child,
            &mut pending,
            &mut pending_stderr,
            &mut frame_scanner,
            None,
            false,
        )
        .await;

        assert!(
            matches!(result, Err(DbOperationError::ObjectMissing(details)) if details.contains("missing_column"))
        );
    }
}
