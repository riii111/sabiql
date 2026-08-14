use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::app::ports::outbound::DbOperationError;

use super::xml::{
    MysqlResultsetFrameScanner, take_mysql_resultset_frame_after_error_check, trace_mysql_frame,
};

pub(super) struct MysqlExportPipeSource<'a, O, E> {
    pub(super) stdout: &'a mut O,
    pub(super) stderr: &'a mut E,
    pub(super) pending: &'a mut Vec<u8>,
    pub(super) error_output: Vec<u8>,
    pub(super) stderr_buffer: [u8; 4096],
    pub(super) stderr_closed: bool,
    pub(super) stdout_closed: bool,
}

impl<O, E> MysqlExportPipeSource<'_, O, E>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    fn capture_error(&mut self, bytes: &[u8]) {
        let remaining = (32usize * 1024).saturating_sub(self.error_output.len());
        if remaining > 0 {
            self.error_output
                .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
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

impl<O, E> AsyncRead for MysqlExportPipeSource<'_, O, E>
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
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        let filled_before = buffer.filled().len();
        let result = Pin::new(&mut *this.stdout).poll_read(cx, buffer);
        if matches!(&result, Poll::Ready(Ok(()))) {
            let count = buffer.filled().len() - filled_before;
            if count == 0 {
                this.stdout_closed = true;
                if !this.stderr_closed {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        } else if matches!(&result, Poll::Pending) && stderr_count > 0 {
            cx.waker().wake_by_ref();
        }
        result
    }
}

pub(super) async fn read_all<R>(reader: &mut R) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await?;
    Ok(output)
}

pub(super) async fn read_one_mysql_resultset_from_pipes<R, E>(
    reader: &mut R,
    stderr: &mut E,
    pending: &mut Vec<u8>,
    pending_stderr: &mut Vec<u8>,
    frame_scanner: &mut MysqlResultsetFrameScanner,
) -> Result<Vec<u8>, DbOperationError>
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
            take_mysql_resultset_frame_after_error_check(pending, pending_stderr, frame_scanner)?
        {
            trace_mysql_frame("receive resultset", frame.len());
            return Ok(frame);
        }
        if stderr_closed {
            let count = reader
                .read(&mut chunk)
                .await
                .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
            if count == 0 {
                return Err(DbOperationError::EmptyResponse(
                    "mysql mode probe returned no resultset".to_string(),
                ));
            }
            pending.extend_from_slice(&chunk[..count]);
        } else {
            tokio::select! {
                result = reader.read(&mut chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        return Err(DbOperationError::EmptyResponse(
                            "mysql mode probe returned no resultset".to_string(),
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

#[cfg(test)]
#[cfg(not(unix))]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::super::error::{classify_mysql_query_failure, has_mysql_cli_error};
    use super::*;

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
        let mut frame_scanner = MysqlResultsetFrameScanner::default();

        let result = read_one_mysql_resultset_from_pipes(
            &mut stdout_reader,
            &mut stderr_reader,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut frame_scanner,
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
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

        let mut source = MysqlExportPipeSource {
            stdout: &mut stdout_reader,
            stderr: &mut stderr_reader,
            pending: &mut Vec::new(),
            error_output: Vec::new(),
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
}
