use std::io;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

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
