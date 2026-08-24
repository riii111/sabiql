use super::*;
use crate::app::ports::outbound::ConnectionFailureKind;

#[tokio::test]
async fn diagnostics_use_adhoc_args_and_follow_resultset_to_marker() {
    let (_directory, program, log_file) = fake_mysql_single_with_warning();
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_diagnostics_with_program(
        OsStr::new(&program),
        &option_file,
        "EXPLAIN FORMAT=TREE SELECT 1",
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .unwrap();

    assert_eq!(
        result.result_set.unwrap().values[0][0].as_str(),
        Some("tree")
    );
    assert_eq!(
        result.diagnostics,
        vec![DatabaseDiagnostic {
            level: DiagnosticLevel::Warning,
            code: 1265,
            message: "truncated".to_string(),
        }]
    );
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(log.contains("--show-warnings"), "{log}");
    assert!(log.contains("EXPLAIN FORMAT=TREE SELECT 1"), "{log}");
    assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN), "{log}");
}

#[tokio::test]
async fn sends_user_sql_only_after_a_valid_session_configuration() {
    let (_directory, program, log_file) = fake_mysql("success");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .unwrap();

    assert_eq!(result.columns, vec!["value"]);
    assert_eq!(result.values[0][0].as_str(), Some("ok"));
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    let positions = [
        MYSQL_SESSION_SETTINGS,
        MYSQL_SESSION_MARKER_COLUMN,
        "__sabiql_sql_mode",
        "SELECT 123",
    ]
    .into_iter()
    .map(|query| log.find(query).expect("query in transcript"))
    .collect::<Vec<_>>();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
    assert!(!log.contains(MYSQL_READ_ONLY_STATEMENT));
}

#[tokio::test]
async fn read_only_session_failure_never_writes_user_sql() {
    let (_directory, program, log_file) = fake_mysql("read_only_failure");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadOnly,
        Duration::from_secs(5),
    )
    .await;

    assert!(result.is_err());
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
    assert!(!log.contains("SELECT 123"), "{log}");
}

#[tokio::test]
async fn nonzero_cli_exit_discards_any_collected_stdout() {
    let (_directory, program, log_file) = fake_mysql("failure");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
}

#[tokio::test]
async fn classifies_cli_error_when_no_resultset_is_emitted() {
    let (_directory, program, log_file) = fake_mysql("no_result_failure");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::ObjectMissing(details))
            if details.contains("missing_column")
    ));
}

#[tokio::test]
async fn classifies_connection_refusal_from_the_shared_cli_error_path() {
    let (_directory, program, log_file) = fake_mysql("connection_refused");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::ConnectionFailedWithKind {
            kind: ConnectionFailureKind::ConnectionRefused,
            ..
        })
    ));
}
