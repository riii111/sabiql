use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::outbound::DbOperationError;

#[derive(Debug, Clone)]
pub(super) struct SqliteCli {
    timeout_secs: u64,
}

struct SqliteOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl SqliteCli {
    pub(super) fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub(super) async fn execute_json<T: DeserializeOwned>(
        &self,
        path: &str,
        sql: &str,
    ) -> Result<T, DbOperationError> {
        let output = self.run(path, sql).await?;
        if !output.status.success() {
            return Err(DbOperationError::QueryFailed(output.stderr));
        }
        let stdout = match output.stdout.trim() {
            "" => "[]",
            stdout => stdout,
        };
        serde_json::from_str(stdout).map_err(DbOperationError::from)
    }

    async fn run(&self, path: &str, sql: &str) -> Result<SqliteOutput, DbOperationError> {
        let mut cmd = Command::new("sqlite3");
        cmd.arg("-readonly").arg("-json").arg(path).arg(sql);
        Self::collect_output(&mut cmd, self.timeout_secs).await
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
