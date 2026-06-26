use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::outbound::DbOperationError;

use super::error::classify_query_error;

pub(in crate::adapters::sqlite) const BUSY_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone)]
pub(in crate::adapters::sqlite) struct SqliteCli {
    timeout_secs: u64,
}

struct SqliteOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl SqliteCli {
    pub(in crate::adapters::sqlite) fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub(in crate::adapters::sqlite) async fn execute_json<T: DeserializeOwned>(
        &self,
        path: &str,
        sql: &str,
    ) -> Result<T, DbOperationError> {
        let output = self.run(path, &["-json"], sql, true).await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        let stdout = match output.stdout.trim() {
            "" => "[]",
            stdout => stdout,
        };
        serde_json::from_str(stdout).map_err(DbOperationError::from)
    }

    pub(in crate::adapters::sqlite) async fn execute_csv(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self
            .run(
                path,
                &["-batch", "-bail", "-csv", "-header"],
                sql,
                read_only,
            )
            .await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
    }

    pub(in crate::adapters::sqlite) async fn execute_quote(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self
            .run(
                path,
                &["-batch", "-bail", "-quote", "-header"],
                sql,
                read_only,
            )
            .await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
    }

    pub(in crate::adapters::sqlite) async fn export_csv(
        &self,
        path: &str,
        sql: &str,
        output_path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let mut cmd = Command::new("sqlite3");
        Self::apply_session_options(&mut cmd, read_only);
        cmd.arg("-batch").arg("-bail").arg("-csv").arg("-header");
        cmd.arg("--").arg(path).arg(sql);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| DbOperationError::CommandNotFound(error.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(output_path)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let mut writer = tokio::io::BufWriter::new(file);

        let result = timeout(Duration::from_secs(self.timeout_secs * 10), async {
            let mut newline_count = 0usize;
            if let Some(mut stdout) = stdout {
                let mut buf = [0u8; 8192];
                loop {
                    let n = stdout.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    newline_count += buf[..n].iter().filter(|&&b| b == b'\n').count();
                    writer.write_all(&buf[..n]).await?;
                }
                writer.flush().await?;
            }

            let stderr = {
                let mut buf = Vec::new();
                if let Some(ref mut stderr) = stderr_handle {
                    stderr.read_to_end(&mut buf).await?;
                }
                String::from_utf8_lossy(&buf).into_owned()
            };
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stderr, newline_count))
        })
        .await;

        let (status, stderr, newline_count) = match result {
            Ok(inner) => inner.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?,
            Err(error) => {
                let _ = tokio::fs::remove_file(output_path).await;
                return Err(DbOperationError::Timeout(error.to_string()));
            }
        };

        if !status.success() {
            let _ = tokio::fs::remove_file(output_path).await;
            return Err(classify_query_error(&stderr));
        }

        Ok(newline_count.saturating_sub(1))
    }

    async fn run(
        &self,
        path: &str,
        args: &[&str],
        sql: &str,
        read_only: bool,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut cmd = Command::new("sqlite3");
        Self::apply_session_options(&mut cmd, read_only);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.arg("--").arg(path).arg(sql);
        Self::collect_output(&mut cmd, self.timeout_secs).await
    }

    fn apply_session_options(cmd: &mut Command, read_only: bool) {
        if read_only {
            cmd.arg("-readonly");
        }
        cmd.arg("-cmd")
            .arg(format!(".timeout {BUSY_TIMEOUT_MS}"))
            .arg("-cmd")
            .arg("PRAGMA foreign_keys=ON");
        if read_only {
            cmd.arg("-cmd").arg("PRAGMA query_only=ON");
        }
    }

    async fn collect_output(
        cmd: &mut Command,
        timeout_secs: u64,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| DbOperationError::CommandNotFound(error.to_string()))?;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(timeout_secs), async {
            let (stdout_result, stderr_result) = tokio::join!(
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut stdout) = stdout_handle {
                        stdout.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut stderr) = stderr_handle {
                        stderr.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await
        .map_err(|error| DbOperationError::Timeout(error.to_string()))?
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

        let (status, stdout, stderr) = result;
        Ok(SqliteOutput {
            status,
            stdout,
            stderr,
        })
    }
}
