use super::process::MySqlProcess;

#[cfg(all(unix, feature = "test-support"))]
use std::ffi::OsStr;
#[cfg(all(unix, feature = "test-support"))]
use std::io;
#[cfg(all(unix, feature = "test-support"))]
use std::path::Path;
#[cfg(all(unix, feature = "test-support"))]
use std::time::Duration;

#[cfg(all(unix, feature = "test-support"))]
use crate::app::ports::outbound::DbOperationError;

#[cfg(all(unix, feature = "test-support"))]
use super::process::{cleanup_mysql_process, stop_mysql_process, write_mysql_input};
#[cfg(all(unix, feature = "test-support"))]
use super::pty::{MySqlPty, read_pty_until_idle_from};
#[cfg(all(unix, feature = "test-support"))]
use super::xml::{trace_mysql_frame, trace_mysql_statement};

#[cfg(all(unix, feature = "test-support"))]
async fn read_pty_until_first_byte_then_idle(
    pty: &mut MySqlPty,
    first_byte_timeout: Duration,
) -> io::Result<Vec<u8>> {
    let output = std::mem::take(&mut pty.pending);
    pty.frame_scanner.reset();
    read_pty_until_idle_from(&mut pty.output, output, true, Some(first_byte_timeout)).await
}

#[cfg(all(unix, feature = "test-support"))]
pub(in crate::adapters::mysql) async fn run_mysql_cli_script_with_program(
    program: &OsStr,
    option_file: &Path,
    script: &str,
    first_byte_timeout: Duration,
) -> Result<Vec<u8>, DbOperationError> {
    let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
    run_mysql_cli_script_process(&mut process, script, first_byte_timeout).await
}

#[cfg(all(unix, feature = "test-support"))]
async fn run_mysql_cli_script_process(
    process: &mut MySqlProcess,
    script: &str,
    first_byte_timeout: Duration,
) -> Result<Vec<u8>, DbOperationError> {
    let result = async {
        trace_mysql_statement(script);
        write_mysql_input(process, script.as_bytes()).await?;
        write_mysql_input(process, b"\x04").await?;
        let output = read_pty_until_first_byte_then_idle(&mut process.pty, first_byte_timeout)
            .await
            .map_err(|error| {
                if error.kind() == io::ErrorKind::TimedOut {
                    DbOperationError::Timeout(error.to_string())
                } else {
                    DbOperationError::QueryFailed(error.to_string())
                }
            })?;
        trace_mysql_frame("receive script output", output.len());
        Ok(output)
    }
    .await;
    if result.is_err() {
        cleanup_mysql_process(process).await;
    } else {
        let _ = stop_mysql_process(&mut process.child).await;
    }
    result
}

#[cfg(all(test, unix, feature = "test-support"))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    fn fake_mysql_without_output() -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        fs::write(&program, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, option_file)
    }

    async fn run_mysql_cli_script_with_program_and_pid(
        program: &Path,
        option_file: &Path,
        script: &str,
        first_byte_timeout: Duration,
    ) -> Result<(u32, Result<Vec<u8>, DbOperationError>), DbOperationError> {
        let mut process = MySqlProcess::spawn_with_program(program.as_os_str(), option_file)?;
        let pid = process.child.id().ok_or_else(|| {
            DbOperationError::ConnectionLost(
                "mysql child exited before cleanup tracking".to_string(),
            )
        })?;
        let result = run_mysql_cli_script_process(&mut process, script, first_byte_timeout).await;
        Ok((pid, result))
    }

    #[tokio::test]
    async fn initial_pty_output_timeout_kills_and_reaps_the_process() {
        let (_directory, program, option_file) = fake_mysql_without_output();
        let (pid, result) = run_mysql_cli_script_with_program_and_pid(
            &program,
            &option_file,
            "SELECT 123;\n",
            Duration::from_millis(50),
        )
        .await
        .expect("spawn script process");

        match result {
            Err(DbOperationError::Timeout(details)) => {
                assert!(
                    details.contains("initial MySQL PTY output wait"),
                    "{details}"
                );
            }
            result => panic!("expected initial PTY output timeout, got {result:?}"),
        }
        let status = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("check script process");
        assert!(!status.success(), "script process {pid} is still running");
    }
}
