use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::app::ports::outbound::DbOperationError;

use super::error::{classify_mysql_query_failure, has_mysql_cli_error, trace_mysql_error};
use super::xml::{
    MysqlResultsetFrameScanner, find_bytes, take_mysql_pty_resultset_frame, trace_mysql_frame,
};

pub(super) struct MysqlPty {
    pub(super) input: TokioFile,
    pub(super) output: TokioFile,
    pub(super) pending: Vec<u8>,
    pub(super) frame_scanner: MysqlResultsetFrameScanner,
}

pub(super) fn create_mysql_pty() -> io::Result<(std::fs::File, std::fs::File)> {
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
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON);
        termios.c_oflag &= !libc::OPOST;
        termios.c_cc[libc::VMIN] = 1;
        termios.c_cc[libc::VTIME] = 0;
        let _ =
            unsafe { libc::tcsetattr(slave_file.as_raw_fd(), libc::TCSANOW, &raw const termios) };
    }
    Ok((master_file, slave_file))
}

pub(super) async fn read_one_pty_resultset(
    pty: &mut MysqlPty,
) -> Result<Vec<u8>, DbOperationError> {
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

pub(super) async fn read_pty_all(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
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

pub(super) async fn read_pty_until_idle(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
    let output = std::mem::take(&mut pty.pending);
    pty.frame_scanner.reset();
    read_pty_until_idle_from(&mut pty.output, output).await
}

async fn read_pty_until_idle_from<R>(reader: &mut R, mut output: Vec<u8>) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0; 4096];
    loop {
        if output.is_empty() {
            match reader.read(&mut chunk).await {
                Ok(0) => return Ok(output),
                Ok(count) => output.extend_from_slice(&chunk[..count]),
                Err(error) if matches!(error.raw_os_error(), Some(libc::EIO | libc::EPERM)) => {
                    return Ok(output);
                }
                Err(error) => return Err(error),
            }
        } else {
            let read =
                tokio::time::timeout(Duration::from_millis(100), reader.read(&mut chunk)).await;
            match read {
                Err(_) | Ok(Ok(0)) => return Ok(output),
                Ok(Ok(count)) => output.extend_from_slice(&chunk[..count]),
                Ok(Err(error)) if matches!(error.raw_os_error(), Some(libc::EIO | libc::EPERM)) => {
                    return Ok(output);
                }
                Ok(Err(error)) => return Err(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_first_pty_byte_before_using_idle_timeout() {
        let (mut writer, mut reader) = tokio::io::duplex(1);
        let read_task =
            tokio::spawn(async move { read_pty_until_idle_from(&mut reader, Vec::new()).await });

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;
        writer.write_all(b"frame").await.unwrap();
        writer.shutdown().await.unwrap();

        assert_eq!(read_task.await.unwrap().unwrap(), b"frame");
    }
}

pub(super) struct MysqlExportPtySource<'a> {
    pub(super) pty: &'a mut MysqlPty,
    pub(super) error_output: Vec<u8>,
    pub(super) error_buffer: Vec<u8>,
    pub(super) pending: Vec<u8>,
    pub(super) started: bool,
}

impl MysqlExportPtySource<'_> {
    fn capture_error(&mut self, bytes: &[u8]) {
        if !self.error_output.is_empty() {
            return;
        }
        self.error_buffer.extend_from_slice(bytes);
        if self.error_buffer.len() > 32 * 1024 {
            let discard = self.error_buffer.len() - 32 * 1024;
            self.error_buffer.drain(..discard);
        }
        if has_mysql_cli_error(&self.error_buffer) {
            self.error_output.extend_from_slice(&self.error_buffer);
        }
    }

    fn discard_before_resultset(&mut self) {
        const RESULTSET_START: &[u8] = b"<resultset";
        if let Some(start) = find_bytes(&self.pending, RESULTSET_START) {
            self.pending.drain(..start);
            self.started = true;
            self.error_buffer.clear();
        } else {
            let keep = RESULTSET_START.len().saturating_sub(1);
            let discard = self.pending.len().saturating_sub(keep);
            self.pending.drain(..discard);
        }
    }
}

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

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[tokio::test]
    async fn captures_mysql_error_split_across_pty_reads() {
        let (master, slave) = create_mysql_pty().expect("create test PTY");
        let output = TokioFile::from_std(master.try_clone().expect("clone PTY master"));
        let input = TokioFile::from_std(master);
        let mut pty = MysqlPty {
            input,
            output,
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
        };
        let mut writer = TokioFile::from_std(slave);
        let producer = tokio::spawn(async move {
            writer.write_all(b"ERROR 1").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            writer
                .write_all(b"054 (42S22): Unknown column missing_column\n<resultset></resultset>")
                .await
                .unwrap();
        });
        let mut source = MysqlExportPtySource {
            pty: &mut pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            started: false,
        };
        let mut output = vec![0; 1024];
        let count = source.read(&mut output).await.unwrap();
        producer.await.unwrap();

        assert!(
            std::str::from_utf8(&output[..count])
                .unwrap()
                .contains("<resultset>")
        );
        assert!(matches!(
            classify_mysql_query_failure(&source.error_output),
            DbOperationError::ObjectMissing(details) if details.contains("missing_column")
        ));
    }

    #[tokio::test]
    async fn captures_mysql_error_split_after_resultset_start() {
        let (master, slave) = create_mysql_pty().expect("create test PTY");
        let output = TokioFile::from_std(master.try_clone().expect("clone PTY master"));
        let input = TokioFile::from_std(master);
        let mut pty = MysqlPty {
            input,
            output,
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
        };
        let mut writer = TokioFile::from_std(slave);
        let producer = tokio::spawn(async move {
            writer.write_all(b"<resultset></resultset>").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            writer.write_all(b"ERROR 1").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            writer
                .write_all(b"054 (42S22): Unknown column missing_column")
                .await
                .unwrap();
        });
        let mut source = MysqlExportPtySource {
            pty: &mut pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            started: false,
        };
        let mut output = vec![0; 1024];
        let count = source.read(&mut output).await.unwrap();
        assert!(
            std::str::from_utf8(&output[..count])
                .unwrap()
                .contains("<resultset>")
        );

        assert!(source.read(&mut output).await.unwrap() > 0);
        assert!(source.read(&mut output).await.unwrap() > 0);
        producer.await.unwrap();

        assert!(matches!(
            classify_mysql_query_failure(&source.error_output),
            DbOperationError::ObjectMissing(details) if details.contains("missing_column")
        ));
    }
}
