#[cfg(not(unix))]
use std::io;

use crate::app::ports::outbound::DatabaseCli;

use super::*;

#[test]
fn maps_missing_mysql_cli_to_command_not_found() {
    let error = map_mysql_cli_spawn_error(io::Error::new(
        io::ErrorKind::NotFound,
        "mysql executable was not found",
    ));

    assert!(matches!(
        error,
        DbOperationError::CommandNotFound {
            command: DatabaseCli::MySql,
            details,
        } if details == "mysql executable was not found"
    ));
}

#[test]
fn maps_other_mysql_cli_spawn_errors_to_connection_failed() {
    let error = map_mysql_cli_spawn_error(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "mysql executable permission denied",
    ));

    assert!(matches!(
        error,
        DbOperationError::ConnectionFailed(details)
            if details == "mysql executable permission denied"
    ));
}

#[test]
fn keeps_production_query_timeout_at_31_seconds() {
    assert_eq!(MYSQL_QUERY_TIMEOUT, Duration::from_secs(31));
}

#[cfg(unix)]
fn session(status: i32, forcibly_stopped: bool, error_bytes: &[u8]) -> MySqlSessionResult {
    MySqlSessionResult {
        status: ExitStatus::from_raw(status),
        forcibly_stopped,
        error_bytes: error_bytes.to_vec(),
    }
}

#[cfg(unix)]
#[test]
fn preserves_mysql_session_exit_rules() {
    assert!(validate_mysql_session_exit(&session(0, false, b""), None).is_ok());
    assert!(matches!(
        validate_mysql_session_exit(
            &session(
                0,
                true,
                b"ERROR 1054 (42S22): Unknown column missing_column"
            ),
            None,
        ),
        Err(DbOperationError::ObjectMissing(_))
    ));
    assert!(validate_mysql_session_exit(&session(1, false, b""), None).is_err());
    assert!(validate_mysql_session_exit(&session(9, true, b""), None).is_ok());
    assert!(matches!(
        validate_mysql_session_exit(
            &session(
                1,
                false,
                b"ERROR 2020 (HY000): Got packet bigger than 'max_allowed_packet' bytes",
            ),
            Some(33_554_432),
        ),
        Err(DbOperationError::QueryFailed(details))
            if details == "MySQL protocol packet exceeds the 33554432-byte client limit"
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn reports_successful_kill_and_wait_status() {
    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep process");

    let (status, forcibly_stopped) = stop_mysql_process(&mut child).await.unwrap();

    assert!(!status.success());
    assert!(forcibly_stopped);
}

#[cfg(unix)]
#[tokio::test]
async fn preserves_wait_status_for_normal_exit() {
    let mut child = Command::new("sh")
        .args(["-c", "exit 7"])
        .spawn()
        .expect("spawn exiting process");
    let expected_status = child.wait().await.unwrap();

    let (status, forcibly_stopped) = stop_mysql_process(&mut child).await.unwrap();

    assert_eq!(status.code(), expected_status.code());
    assert!(!forcibly_stopped);
}

#[cfg(unix)]
#[tokio::test]
async fn failed_kill_preserves_wait_status_without_forced_stop() {
    let mut child = Command::new("sh")
        .args(["-c", "exit 7"])
        .spawn()
        .expect("spawn exiting process");
    let expected_status = child.wait().await.unwrap();
    let (status, forcibly_stopped) =
        finish_mysql_process_stop(&mut child, Err(std::io::Error::other("kill failed")))
            .await
            .unwrap();

    assert_eq!(status.code(), expected_status.code());
    assert!(!forcibly_stopped);
    assert!(
        validate_mysql_session_exit(
            &MySqlSessionResult {
                status,
                forcibly_stopped,
                error_bytes: Vec::new(),
            },
            None,
        )
        .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn successful_kill_request_does_not_override_normal_exit_status() {
    let mut child = Command::new("sh")
        .args(["-c", "exit 7"])
        .spawn()
        .expect("spawn exiting process");
    let expected_status = child.wait().await.unwrap();
    let (status, forcibly_stopped) = finish_mysql_process_stop(&mut child, Ok(())).await.unwrap();

    assert_eq!(status.code(), expected_status.code());
    assert!(!forcibly_stopped);
    assert!(
        validate_mysql_session_exit(
            &MySqlSessionResult {
                status,
                forcibly_stopped,
                error_bytes: Vec::new(),
            },
            None,
        )
        .is_err()
    );
}

#[test]
fn separates_statements_after_line_comments_with_semicolons() {
    for query in [
        "SELECT 1",
        "SELECT 1 -- trailing comment;",
        "SELECT 1 # trailing comment;",
    ] {
        assert_eq!(
            mysql_statement_input(query),
            format!("{query}\n;\n").into_bytes()
        );
    }
}

#[test]
fn adds_an_independent_separator_after_an_existing_semicolon() {
    assert_eq!(mysql_statement_input("SELECT 1;"), b"SELECT 1;\n;\n");
}
