use super::*;

#[tokio::test]
async fn failure_never_writes_user_sql() {
    for mode in ["unsupported", "invalid", "missing"] {
        let (_directory, program, log_file) = fake_mysql(mode);
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
        assert!(result.is_err(), "{mode}");
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
        assert!(!log.contains("SELECT 123"), "{mode}: {log}");
    }
}

#[tokio::test]
async fn timeout_kills_the_process_and_discards_output() {
    let (_directory, program, log_file) = fake_mysql("timeout");
    let option_file = log_file.with_extension("cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let result = run_mysql_single_statement_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 123",
        AccessMode::ReadWrite,
        Duration::from_millis(50),
    )
    .await;

    assert!(matches!(result, Err(DbOperationError::Timeout(_))));
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
    assert!(!log.contains("SELECT 123"));
}
