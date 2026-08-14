#[cfg(not(unix))]
struct MysqlExportPipeSource<'a, O, E> {
    stdout: &'a mut O,
    stderr: &'a mut E,
    pending: &'a mut Vec<u8>,
    error_output: Vec<u8>,
    stderr_buffer: [u8; 4096],
    stderr_closed: bool,
    stdout_closed: bool,
}

#[cfg(not(unix))]
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

#[cfg(not(unix))]
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

#[cfg(not(unix))]
async fn read_all<R>(reader: &mut R) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await?;
    Ok(output)
}

#[cfg(not(unix))]
async fn read_one_mysql_resultset_from_pipes<R, E>(
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
