use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::app::ports::outbound::DbOperationError;

use super::error::{classify_mysql_query_failure, has_mysql_cli_error, trace_mysql_error};
use super::xml::{MysqlResultsetFrameScanner, take_mysql_pty_resultset_frame, trace_mysql_frame};

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
    read_pty_until_idle_from(&mut pty.output, output, false).await
}

pub(super) async fn read_pty_until_first_byte_then_idle(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
    let output = std::mem::take(&mut pty.pending);
    pty.frame_scanner.reset();
    read_pty_until_idle_from(&mut pty.output, output, true).await
}

async fn read_pty_until_idle_from<R>(
    reader: &mut R,
    mut output: Vec<u8>,
    wait_for_first_byte: bool,
) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0; 4096];
    loop {
        if wait_for_first_byte && output.is_empty() {
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

pub(super) struct MysqlExportPtySource<'a> {
    pub(super) pty: &'a mut MysqlPty,
    pub(super) error_output: Vec<u8>,
    pub(super) error_buffer: Vec<u8>,
    pub(super) pending: Vec<u8>,
    pub(super) frame_scanner: MysqlResultsetFrameScanner,
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

    fn append_before_resultset(&mut self, bytes: &[u8]) {
        if self.started {
            self.pending.extend_from_slice(bytes);
            return;
        }

        let previous_len = self.pending.len();
        self.pending.extend_from_slice(bytes);
        let start = self.frame_scanner.frame_start(&self.pending);
        let prefix_end = start.unwrap_or(self.pending.len());
        if prefix_end > previous_len {
            let prefix = self.pending[previous_len..prefix_end].to_vec();
            self.capture_error(&prefix);
        }
        if let Some(start) = start {
            self.pending.drain(..start);
            self.started = true;
            self.error_buffer.clear();
        } else {
            let keep = b"<resultset".len().saturating_sub(1);
            let discard = self.pending.len().saturating_sub(keep);
            self.pending.drain(..discard);
            self.frame_scanner.reset();
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
                    this.append_before_resultset(&bytes);
                }
                if this.started {
                    continue;
                }

                let mut chunk = [0; 4096];
                let mut read_buffer = ReadBuf::new(&mut chunk);
                match Pin::new(&mut this.pty.output).poll_read(cx, &mut read_buffer) {
                    Poll::Ready(Ok(())) => {
                        let bytes = read_buffer.filled().to_vec();
                        if bytes.is_empty() {
                            return Poll::Ready(Ok(()));
                        }
                        this.append_before_resultset(&bytes);
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
                return Pin::new(&mut this.pty.output).poll_read(cx, buffer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Seek, SeekFrom, Write};

    use tokio::fs::File as TokioFile;
    use tokio::io::AsyncWriteExt;

    use super::*;

    fn source_with_output(output: &[u8]) -> (tempfile::NamedTempFile, MysqlPty) {
        let mut output_file = tempfile::NamedTempFile::new().unwrap();
        output_file.write_all(output).unwrap();
        output_file.as_file_mut().seek(SeekFrom::Start(0)).unwrap();
        let input_file = tempfile::NamedTempFile::new().unwrap();
        let pty = MysqlPty {
            input: TokioFile::from_std(input_file.reopen().unwrap()),
            output: TokioFile::from_std(output_file.reopen().unwrap()),
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
        };
        (output_file, pty)
    }

    #[tokio::test]
    async fn export_source_ignores_cli_error_text_inside_resultset() {
        let xml = br#"<resultset><row><field name="message">line 1
ERROR 1146 (42S02): this is a cell value</field></row></resultset>"#;
        let (_output_file, mut pty) = source_with_output(xml);
        let mut source = MysqlExportPtySource {
            pty: &mut pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
            started: false,
        };
        let mut result = Vec::new();

        source.read_to_end(&mut result).await.unwrap();

        assert_eq!(result, xml);
        assert!(!has_mysql_cli_error(&source.error_output));
    }

    #[tokio::test]
    async fn export_source_keeps_cli_error_before_resultset() {
        let output = b"ERROR 1054 (42S22): Unknown column\n<resultset></resultset>";
        let (_output_file, mut pty) = source_with_output(output);
        let mut source = MysqlExportPtySource {
            pty: &mut pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
            started: false,
        };
        let mut result = Vec::new();

        source.read_to_end(&mut result).await.unwrap();

        assert!(has_mysql_cli_error(&source.error_output));
    }

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
            frame_scanner: MysqlResultsetFrameScanner::default(),
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
    async fn does_not_capture_cli_error_text_after_resultset_start() {
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
            frame_scanner: MysqlResultsetFrameScanner::default(),
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

        assert!(source.error_output.is_empty());
    }

    #[test]
    fn rescans_resultset_marker_after_draining_large_pty_reads() {
        let (_output_file, mut pty) = source_with_output(&[]);
        let mut source = MysqlExportPtySource {
            pty: &mut pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            frame_scanner: MysqlResultsetFrameScanner::default(),
            started: false,
        };
        let mut first = vec![b'x'; 4096 - b"<resultse".len()];
        first.extend_from_slice(b"<resultse");
        let mut second = vec![b'y'; 4096];
        second[..b"t></resultset>".len()].copy_from_slice(b"t></resultset>");

        source.append_before_resultset(&first);
        source.append_before_resultset(&second);

        assert!(source.started);
        assert!(source.pending.starts_with(b"<resultset>"));
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_zero_byte_idle_timeout_for_production_pty_reads() {
        let (_writer, mut reader) = tokio::io::duplex(1);
        let read_task =
            tokio::spawn(
                async move { read_pty_until_idle_from(&mut reader, Vec::new(), false).await },
            );

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;

        assert!(read_task.await.unwrap().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_first_pty_byte_before_using_idle_timeout() {
        let (mut writer, mut reader) = tokio::io::duplex(1);
        let read_task =
            tokio::spawn(
                async move { read_pty_until_idle_from(&mut reader, Vec::new(), true).await },
            );

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;
        writer.write_all(b"frame").await.unwrap();
        writer.shutdown().await.unwrap();

        assert_eq!(read_task.await.unwrap().unwrap(), b"frame");
    }
}
