use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::app::ports::outbound::DbOperationError;

use super::error::{classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error};
use super::process::read_all_bytes;
use super::xml::{
    MySqlResultSetFrameScanner,
    take_mysql_resultset_frame_after_error_check_with_diagnostics_and_preview_budget,
    trace_mysql_frame,
};

pub(super) struct MySqlExportPipeSource<'a, O, E> {
    pub(super) stdout: &'a mut O,
    pub(super) stderr: &'a mut E,
    pub(super) pending: &'a mut Vec<u8>,
    pub(super) client_packet_limit_bytes: Option<usize>,
    pub(super) error_output: Vec<u8>,
    pub(super) error_buffer: Vec<u8>,
    pub(super) stderr_buffer: [u8; 4096],
    pub(super) stderr_closed: bool,
    pub(super) stdout_closed: bool,
}

impl<O, E> MySqlExportPipeSource<'_, O, E>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    pub(super) fn capture_error(&mut self, bytes: &[u8]) {
        if !self.error_output.is_empty() {
            return;
        }
        self.error_buffer.extend_from_slice(bytes);
        if self.error_buffer.len() > 32 * 1024 {
            let discard = self.error_buffer.len() - 32 * 1024;
            self.error_buffer.drain(..discard);
        }
        if has_complete_mysql_cli_error(&self.error_buffer) {
            self.error_output.extend_from_slice(&self.error_buffer);
        }
    }

    fn finish_error_capture(&mut self) {
        if self.error_output.is_empty() && has_mysql_cli_error(&self.error_buffer) {
            self.error_output.extend_from_slice(&self.error_buffer);
        }
    }

    fn poll_stderr(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        if self.stderr_closed {
            return Poll::Ready(Ok(0));
        }
        let result = {
            let mut read_buffer = ReadBuf::new(&mut self.stderr_buffer);
            match Pin::new(&mut *self.stderr).poll_read(cx, &mut read_buffer) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buffer.filled().to_vec())),
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            }
        };
        match result {
            Poll::Ready(Ok(bytes)) => {
                if bytes.is_empty() {
                    self.stderr_closed = true;
                    self.finish_error_capture();
                } else {
                    self.capture_error(&bytes);
                }
                Poll::Ready(Ok(bytes.len()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn has_complete_mysql_cli_error(output: &[u8]) -> bool {
    output
        .split_inclusive(|byte| *byte == b'\n' || *byte == b'\r')
        .any(|line| {
            line.last()
                .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
                && has_mysql_cli_error(line)
        })
}

impl<O, E> AsyncRead for MySqlExportPipeSource<'_, O, E>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let stderr_count = match this.poll_stderr(cx) {
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Pending => 0,
            Poll::Ready(Ok(count)) => count,
        };
        if !this.pending.is_empty() {
            let count = buffer.remaining().min(this.pending.len());
            let bytes = this.pending.drain(..count).collect::<Vec<_>>();
            buffer.put_slice(&bytes);
            return Poll::Ready(Ok(()));
        }

        if this.stdout_closed {
            if this.stderr_closed {
                return Poll::Ready(Ok(()));
            }
            if stderr_count > 0 {
                cx.waker().wake_by_ref();
            }
            return Poll::Pending;
        }

        let filled_before = buffer.filled().len();
        let result = Pin::new(&mut *this.stdout).poll_read(cx, buffer);
        if matches!(&result, Poll::Ready(Ok(()))) {
            let count = buffer.filled().len() - filled_before;
            if count == 0 {
                this.stdout_closed = true;
                if !this.stderr_closed {
                    if stderr_count > 0 {
                        cx.waker().wake_by_ref();
                    }
                    return Poll::Pending;
                }
            }
        } else if matches!(&result, Poll::Pending) && stderr_count > 0 {
            cx.waker().wake_by_ref();
        }
        result
    }
}

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
        if stderr_closed {
            let count = reader
                .read(&mut chunk)
                .await
                .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
            if count == 0 {
                finish_mysql_pipe_after_stdout_eof(stderr, child, pending_stderr).await?;
                return Err(mysql_pipe_empty_response_or_error(
                    pending_stderr,
                    client_packet_limit_bytes,
                ));
            }
            pending.extend_from_slice(&chunk[..count]);
        } else {
            tokio::select! {
                result = reader.read(&mut chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        finish_mysql_pipe_after_stdout_eof(stderr, child, pending_stderr).await?;
                        return Err(mysql_pipe_empty_response_or_error(
                            pending_stderr,
                            client_packet_limit_bytes,
                        ));
                    }
                    pending.extend_from_slice(&chunk[..count]);
                }
                result = stderr.read(&mut stderr_chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        stderr_closed = true;
                    } else {
                        pending_stderr.extend_from_slice(&stderr_chunk[..count]);
                    }
                }
            }
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
        DbOperationError::EmptyResponse("mysql mode probe returned no resultset".to_string())
    }
}

#[cfg(test)]
#[cfg(not(unix))]
mod tests {
    use std::process::Stdio;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Wake, Waker};

    use tokio::io::AsyncWriteExt;
    use tokio::process::{Child, Command};

    use super::super::error::classify_mysql_query_failure;
    use super::*;

    struct PendingReader {
        waker: Option<Waker>,
    }

    impl AsyncRead for PendingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            _buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.get_mut().waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    struct WakeCounter(AtomicUsize);

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn does_not_self_wake_while_waiting_for_stderr_after_stdout_eof() {
        let mut stdout = tokio::io::empty();
        let mut stderr = PendingReader { waker: None };
        let mut pending = Vec::new();
        let mut source = MySqlExportPipeSource {
            stdout: &mut stdout,
            stderr: &mut stderr,
            pending: &mut pending,
            client_packet_limit_bytes: None,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let wake_counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&wake_counter));
        let mut context = Context::from_waker(&waker);
        let mut output = [0; 8];
        let mut buffer = ReadBuf::new(&mut output);

        let result = Pin::new(&mut source).poll_read(&mut context, &mut buffer);

        assert!(matches!(result, Poll::Pending));
        assert_eq!(wake_counter.0.load(Ordering::Relaxed), 0);
    }

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

    #[tokio::test]
    async fn consumes_stderr_while_streaming_stdout() {
        let (mut stdout_writer, mut stdout_reader) = tokio::io::duplex(64);
        let (mut stderr_writer, mut stderr_reader) = tokio::io::duplex(64);
        let stdout = b"<resultset><row><field name=\"value\">ok</field></row></resultset>".to_vec();
        let stderr = format!(
            "ERROR 1146 (42S02): Table 'app.missing' doesn't exist\n{}",
            "warning\n".repeat(16 * 1024)
        )
        .into_bytes();

        let stdout_task = tokio::spawn(async move {
            stdout_writer.write_all(&stdout).await.unwrap();
        });
        let stderr_task = tokio::spawn(async move {
            stderr_writer.write_all(&stderr).await.unwrap();
        });

        let mut source = MySqlExportPipeSource {
            stdout: &mut stdout_reader,
            stderr: &mut stderr_reader,
            pending: &mut Vec::new(),
            client_packet_limit_bytes: None,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let mut output = Vec::new();
        source.read_to_end(&mut output).await.unwrap();
        stdout_task.await.unwrap();
        stderr_task.await.unwrap();

        assert!(output.starts_with(b"<resultset>"));
        assert!(has_mysql_cli_error(&source.error_output));
        assert!(matches!(
            classify_mysql_query_failure(&source.error_output),
            DbOperationError::ObjectMissing(_)
        ));
    }

    #[tokio::test]
    async fn captures_error_after_large_stderr_prefix_and_read_split() {
        let (mut stdout_writer, mut stdout_reader) = tokio::io::duplex(1024);
        let (mut stderr_writer, mut stderr_reader) = tokio::io::duplex(64);
        let stdout_task = tokio::spawn(async move {
            stdout_writer
                .write_all(b"<resultset><row></row></resultset>")
                .await
                .unwrap();
        });
        let stderr_task = tokio::spawn(async move {
            stderr_writer
                .write_all(&b"warning\n".repeat(8 * 1024))
                .await
                .unwrap();
            stderr_writer.write_all(b"ERROR 1").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            stderr_writer
                .write_all(b"054 (42S22): Unknown column missing_column \xe6")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            stderr_writer
                .write_all(b"\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\n")
                .await
                .unwrap();
        });

        let mut source = MySqlExportPipeSource {
            stdout: &mut stdout_reader,
            stderr: &mut stderr_reader,
            pending: &mut Vec::new(),
            client_packet_limit_bytes: None,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let mut output = Vec::new();
        source.read_to_end(&mut output).await.unwrap();
        stdout_task.await.unwrap();
        stderr_task.await.unwrap();

        assert_eq!(output, b"<resultset><row></row></resultset>");
        assert!(matches!(
            classify_mysql_query_failure(&source.error_output),
            DbOperationError::ObjectMissing(details)
                if details.contains("missing_column 日本語")
        ));
    }
}
