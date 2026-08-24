use crate::adapters::csv_export::CsvFileWriter;

use super::super::export::stream_mysql_resultset_to_csv;
use super::*;

#[tokio::test]
async fn csv_stream_returns_incomplete_stderr_for_final_classification() {
    let mut child = Command::new("cmd.exe")
            .args([
                "/C",
                "echo ^<resultset^>^</resultset^> & ping -n 2 127.0.0.1 >nul & echo 054 (42S22): missing_column 1>&2",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn cmd.exe");
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut process = MySqlProcess {
        child,
        client_packet_limit_bytes: None,
        preview_byte_budget: false,
        stdin: Some(stdin),
        stdout,
        stderr,
        pending: Vec::new(),
        pending_stderr: b"ERROR 1".to_vec(),
        frame_scanner: MySqlResultSetFrameScanner::default(),
    };
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("export.csv");
    let mut csv_writer = CsvFileWriter::create(path).await.unwrap();

    assert!(
        stream_mysql_resultset_to_csv(&mut process, &mut csv_writer)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(process.pending_stderr, b"ERROR 1");
    csv_writer.finish().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), finish_mysql_session(&mut process))
        .await
        .expect("finish pipe process timed out")
        .expect("finish pipe process");

    assert!(matches!(
        classify_mysql_query_failure(&result.error_bytes),
        DbOperationError::ObjectMissing(details) if details.contains("missing_column")
    ));
}

#[tokio::test]
async fn pipe_finish_combines_pending_stderr_with_final_read() {
    let mut child = Command::new("cmd.exe")
        .args([
            "/C",
            "findstr /R \"^\" >nul & echo 054 (42S22): missing_column 1>&2 & exit /B 0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cmd.exe");
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut process = MySqlProcess {
        child,
        client_packet_limit_bytes: None,
        preview_byte_budget: false,
        stdin: Some(stdin),
        stdout,
        stderr,
        pending: Vec::new(),
        pending_stderr: b"ERROR 1".to_vec(),
        frame_scanner: MySqlResultSetFrameScanner::default(),
    };

    let result = tokio::time::timeout(Duration::from_secs(2), finish_mysql_session(&mut process))
        .await
        .expect("finish pipe process timed out waiting for stdin EOF")
        .expect("finish pipe process");

    assert!(matches!(
        classify_mysql_query_failure(&result.error_bytes),
        DbOperationError::ObjectMissing(details) if details.contains("missing_column")
    ));
}

#[tokio::test]
async fn pipe_finish_shuts_down_stdin_before_draining_cli_error() {
    let mut child = Command::new("cmd.exe")
        .args([
            "/C",
            "findstr /R \"^\" >nul & echo ERROR 1054 (42S22): missing_column 1>&2 & exit /B 0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cmd.exe");
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut process = MySqlProcess {
        child,
        client_packet_limit_bytes: None,
        preview_byte_budget: false,
        stdin: Some(stdin),
        stdout,
        stderr,
        pending: Vec::new(),
        pending_stderr: Vec::new(),
        frame_scanner: MySqlResultSetFrameScanner::default(),
    };

    let result = tokio::time::timeout(Duration::from_secs(2), finish_mysql_session(&mut process))
        .await
        .expect("finish pipe process timed out waiting for stdin EOF")
        .expect("finish pipe process");

    assert_eq!(result.status.code(), Some(0));
    assert!(!result.forcibly_stopped);
    assert!(has_mysql_cli_error(&result.error_bytes));
    assert!(matches!(
        classify_mysql_query_failure(&result.error_bytes),
        DbOperationError::ObjectMissing(details) if details.contains("missing_column")
    ));
}
