#[cfg(unix)]
struct MysqlPty {
    input: TokioFile,
    output: TokioFile,
    pending: Vec<u8>,
    frame_scanner: MysqlResultsetFrameScanner,
}

#[cfg(unix)]
fn create_mysql_pty() -> io::Result<(std::fs::File, std::fs::File)> {
    let mut master = -1;
    let mut slave = -1;
    let result = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
    let master_file = unsafe { std::fs::File::from_raw_fd(master) };
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(slave_file.as_raw_fd(), termios.as_mut_ptr()) } == 0 {
        let mut termios = unsafe { termios.assume_init() };
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL);
        termios.c_oflag &= !libc::OPOST;
        let _ =
            unsafe { libc::tcsetattr(slave_file.as_raw_fd(), libc::TCSANOW, &raw const termios) };
    }
    Ok((master_file, slave_file))
}

#[cfg(unix)]
async fn read_one_pty_resultset(pty: &mut MysqlPty) -> Result<Vec<u8>, DbOperationError> {
    let mut chunk = [0; 4096];
    loop {
        if let Some(frame) =
            take_mysql_pty_resultset_frame(&mut pty.pending, &mut pty.frame_scanner)?
        {
            trace_mysql_frame("receive resultset", frame.len());
            return Ok(frame);
        }
        let count = match pty.output.read(&mut chunk).await {
            Ok(count) => count,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => 0,
            Err(error) => return Err(DbOperationError::ConnectionLost(error.to_string())),
        };
        if count == 0 {
            let tail = read_pty_all(pty)
                .await
                .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
            if has_mysql_cli_error(&tail) {
                trace_mysql_error(&tail);
                return Err(classify_mysql_query_failure(&tail));
            }
            return Err(DbOperationError::EmptyResponse(
                "mysql query returned no resultset".to_string(),
            ));
        }
        pty.pending.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(unix)]
async fn read_pty_all(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
    let mut output = std::mem::take(&mut pty.pending);
    pty.frame_scanner.reset();
    let mut chunk = [0; 4096];
    loop {
        match pty.output.read(&mut chunk).await {
            Ok(0) => return Ok(output),
            Ok(count) => output.extend_from_slice(&chunk[..count]),
            Err(error) if matches!(error.raw_os_error(), Some(libc::EIO | libc::EPERM)) => {
                return Ok(output);
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
struct MysqlExportPtySource<'a> {
    pty: &'a mut MysqlPty,
    error_output: Vec<u8>,
    pending: Vec<u8>,
    started: bool,
}

#[cfg(unix)]
impl MysqlExportPtySource<'_> {
    fn capture_error(&mut self, bytes: &[u8]) {
        if self.error_output.is_empty() && has_mysql_cli_error(bytes) {
            self.error_output
                .extend_from_slice(&bytes[..bytes.len().min(32 * 1024)]);
        }
    }

    fn discard_before_resultset(&mut self) {
        const RESULTSET_START: &[u8] = b"<resultset";
        if let Some(start) = find_bytes(&self.pending, RESULTSET_START) {
            self.pending.drain(..start);
            self.started = true;
        } else {
            let keep = RESULTSET_START.len().saturating_sub(1);
            let discard = self.pending.len().saturating_sub(keep);
            self.pending.drain(..discard);
        }
    }
}

#[cfg(unix)]
impl AsyncRead for MysqlExportPtySource<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.started {
                if !this.pty.pending.is_empty() {
                    let bytes = std::mem::take(&mut this.pty.pending);
                    this.capture_error(&bytes);
                    this.pending.extend_from_slice(&bytes);
                }
                if !this.pending.is_empty() {
                    this.discard_before_resultset();
                    if this.started {
                        continue;
                    }
                }

                let mut chunk = [0; 4096];
                let mut read_buffer = ReadBuf::new(&mut chunk);
                match Pin::new(&mut this.pty.output).poll_read(cx, &mut read_buffer) {
                    Poll::Ready(Ok(())) => {
                        let bytes = read_buffer.filled().to_vec();
                        if bytes.is_empty() {
                            return Poll::Ready(Ok(()));
                        }
                        this.capture_error(&bytes);
                        this.pending.extend_from_slice(&bytes);
                    }
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => return Poll::Pending,
                }
                continue;
            }

            if !this.pending.is_empty() {
                let count = buffer.remaining().min(this.pending.len());
                let bytes = this.pending.drain(..count).collect::<Vec<_>>();
                buffer.put_slice(&bytes);
                return Poll::Ready(Ok(()));
            }

            {
                let filled_before = buffer.filled().len();
                let result = Pin::new(&mut this.pty.output).poll_read(cx, buffer);
                if matches!(&result, Poll::Ready(Ok(()))) {
                    this.capture_error(&buffer.filled()[filled_before..]);
                }
                return result;
            }
        }
    }
}
