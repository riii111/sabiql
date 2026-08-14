#[cfg(all(test, not(unix)))]
mod pipe_executor_tests {
    use tokio::io::AsyncWriteExt;

    use super::super::*;

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
}
#[cfg(all(test, not(unix)))]
mod export_pipe_tests {
    use tokio::io::AsyncWriteExt;

    use super::super::super::error::{classify_mysql_query_failure, has_mysql_cli_error};
    use super::super::*;

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
