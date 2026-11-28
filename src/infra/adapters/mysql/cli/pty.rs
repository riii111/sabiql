use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::time::Duration;

use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::DatabaseDiagnostic;

use super::error::{
    classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error, trace_mysql_error,
};
use super::xml::{
    MySqlResultSetFrameScanner, take_mysql_pty_resultset_frame_with_diagnostics_and_preview_budget,
    trace_mysql_frame,
};

pub(super) struct MySqlPty {
    pub(super) input: TokioFile,
    pub(super) output: TokioFile,
    pub(super) pending: Vec<u8>,
    pub(super) frame_scanner: MySqlResultSetFrameScanner,
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
    configure_mysql_pty(slave_file.as_raw_fd())?;
    Ok((master_file, slave_file))
}

fn configure_mysql_pty(fd: RawFd) -> io::Result<()> {
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut termios = unsafe { termios.assume_init() };
    unsafe { libc::cfmakeraw(&raw mut termios) };
    termios.c_cc[libc::VMIN] = 1;
    termios.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const termios) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) async fn read_one_pty_resultset(
    pty: &mut MySqlPty,
    client_packet_limit_bytes: Option<usize>,
    preview_byte_budget: bool,
) -> Result<Vec<u8>, DbOperationError> {
    Ok(
        read_one_pty_resultset_with_diagnostics(
            pty,
            client_packet_limit_bytes,
            preview_byte_budget,
        )
        .await?
        .0,
    )
}

pub(super) async fn read_one_pty_resultset_with_diagnostics(
    pty: &mut MySqlPty,
    client_packet_limit_bytes: Option<usize>,
    preview_byte_budget: bool,
) -> Result<(Vec<u8>, Vec<DatabaseDiagnostic>), DbOperationError> {
    let mut chunk = [0; 4096];
    loop {
        if let Some(frame) = take_mysql_pty_resultset_frame_with_diagnostics_and_preview_budget(
            &mut pty.pending,
            &mut pty.frame_scanner,
            client_packet_limit_bytes,
            preview_byte_budget,
        )? {
            trace_mysql_frame("receive resultset", frame.0.len());
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
                return Err(classify_mysql_query_failure_with_packet_limit(
                    &tail,
                    client_packet_limit_bytes,
                ));
            }
            return Err(DbOperationError::QueryFailed(
                "mysql query returned no resultset".to_string(),
            ));
        }
        pty.pending.extend_from_slice(&chunk[..count]);
    }
}

pub(super) async fn read_pty_all(pty: &mut MySqlPty) -> io::Result<Vec<u8>> {
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

pub(super) async fn read_pty_until_idle(pty: &mut MySqlPty) -> io::Result<Vec<u8>> {
    let output = std::mem::take(&mut pty.pending);
    pty.frame_scanner.reset();
    read_pty_until_idle_from(&mut pty.output, output, None).await
}

pub(super) async fn read_pty_until_idle_from<R>(
    reader: &mut R,
    mut output: Vec<u8>,
    initial_byte_timeout: Option<Duration>,
) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0; 4096];
    loop {
        if output.is_empty()
            && let Some(timeout) = initial_byte_timeout
        {
            let read = tokio::time::timeout(timeout, reader.read(&mut chunk))
                .await
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "initial MySQL PTY output wait timed out",
                    )
                })?;
            match read {
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
    use std::io::{Seek, SeekFrom, Write};

    use tokio::fs::File as TokioFile;
    use tokio::io::AsyncWriteExt;

    use super::*;

    fn source_with_output(output: &[u8]) -> (tempfile::NamedTempFile, MySqlPty) {
        let mut output_file = tempfile::NamedTempFile::new().unwrap();
        output_file.write_all(output).unwrap();
        output_file.as_file_mut().seek(SeekFrom::Start(0)).unwrap();
        let input_file = tempfile::NamedTempFile::new().unwrap();
        let pty = MySqlPty {
            input: TokioFile::from_std(input_file.reopen().unwrap()),
            output: TokioFile::from_std(output_file.reopen().unwrap()),
            pending: Vec::new(),
            frame_scanner: MySqlResultSetFrameScanner::default(),
        };
        (output_file, pty)
    }

    #[test]
    fn configures_mysql_pty_as_raw_with_single_byte_reads() {
        let (_master, slave) = create_mysql_pty().expect("create test PTY");
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();

        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), termios.as_mut_ptr()) },
            0
        );
        let termios = unsafe { termios.assume_init() };

        assert_eq!(
            termios.c_iflag
                & (libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON),
            0
        );
        assert_eq!(termios.c_oflag & libc::OPOST, 0);
        assert_eq!(
            termios.c_lflag
                & (libc::ECHO | libc::ECHONL | libc::ICANON | libc::IEXTEN | libc::ISIG),
            0
        );
        assert_eq!(termios.c_cflag & libc::CS8, libc::CS8);
        assert_eq!(termios.c_cc[libc::VMIN], 1);
        assert_eq!(termios.c_cc[libc::VTIME], 0);
    }

    #[test]
    fn reports_termios_get_failure() {
        let error = configure_mysql_pty(-1).expect_err("invalid fd must fail");

        assert_eq!(error.raw_os_error(), Some(libc::EBADF));
    }

    #[tokio::test]
    async fn reads_the_message_after_a_split_mysql_error_header() {
        let header = "ERROR 1792 (25006) at line 7";
        let message = ": Cannot execute statement in a READ ONLY transaction.";
        let (_output_file, mut pty) = source_with_output(format!("{message}\n").as_bytes());
        pty.pending.extend_from_slice(header.as_bytes());

        let result = read_one_pty_resultset(&mut pty, None, false).await;

        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailed(details)) if details == format!("{header}{message}")
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_zero_byte_idle_timeout_for_production_pty_reads() {
        let (_writer, mut reader) = tokio::io::duplex(1);
        let read_task =
            tokio::spawn(
                async move { read_pty_until_idle_from(&mut reader, Vec::new(), None).await },
            );

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;

        assert!(read_task.await.unwrap().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_first_pty_byte_before_using_idle_timeout() {
        let (mut writer, mut reader) = tokio::io::duplex(1);
        let read_task = tokio::spawn(async move {
            read_pty_until_idle_from(&mut reader, Vec::new(), Some(Duration::from_secs(1))).await
        });

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;
        writer.write_all(b"frame").await.unwrap();
        writer.shutdown().await.unwrap();

        assert_eq!(read_task.await.unwrap().unwrap(), b"frame");
    }

    #[tokio::test(start_paused = true)]
    async fn times_out_when_test_support_pty_has_no_initial_output() {
        let (_writer, mut reader) = tokio::io::duplex(1);
        let read_task = tokio::spawn(async move {
            read_pty_until_idle_from(&mut reader, Vec::new(), Some(Duration::from_secs(1))).await
        });

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;

        let error = read_task
            .await
            .unwrap()
            .expect_err("initial output must time out");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "initial MySQL PTY output wait timed out");
    }
}
