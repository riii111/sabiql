use super::*;

#[tokio::test]
async fn executes_each_statement_and_returns_the_last_user_result() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT 2")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_else(|error| panic!("multi execution failed: {error:?}"));

    assert_eq!(
        result.result_set,
        Some(MySqlResultSet {
            columns: vec!["value".to_string()],
            values: vec![vec![QueryValue::Text("two".to_string())]],
        })
    );
    assert_eq!(result.command_tag, None);
    assert_eq!(result.refresh_scope, RefreshScope::Data);
    assert!(result.diagnostics.is_empty());
    let log = fs::read_to_string(log_file).unwrap();
    assert!(log.contains("UPDATE items SET value = 1"));
    assert_eq!(log.matches("__sabiql_marker").count(), 1);
    assert!(!log.contains("ROW_COUNT()"));
}

#[tokio::test]
async fn skips_resultset_wait_for_select_into_user_variable() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements("SELECT id INTO @picked FROM items; SELECT @picked")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .expect("SELECT INTO user variable execution");

    assert_eq!(
        result.result_set,
        Some(MySqlResultSet {
            columns: vec!["value".to_string()],
            values: vec![vec![QueryValue::Text("picked".to_string())]],
        })
    );
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(log.contains("SELECT id INTO @picked FROM items"), "{log}");
    assert!(log.contains("SELECT @picked"), "{log}");
}

#[tokio::test]
async fn keeps_multi_statement_diagnostics_on_the_submission_result() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements(
                "INSERT IGNORE INTO items (id) VALUES (1); CREATE TABLE IF NOT EXISTS items (id INT); SELECT 2",
            )
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .expect("multi-statement diagnostics execution");

    assert_eq!(
        result.result_set.as_ref().map(|result| &result.values),
        Some(&vec![vec![QueryValue::Text("two".to_string())]])
    );
    assert_eq!(
        result.diagnostics,
        vec![
            DatabaseDiagnostic {
                level: DiagnosticLevel::Warning,
                code: 1062,
                message: "duplicate ignored".to_string(),
            },
            DatabaseDiagnostic {
                level: DiagnosticLevel::Note,
                code: 1050,
                message: "table already exists".to_string(),
            },
        ]
    );
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert_eq!(log.matches("__sabiql_marker").count(), 1);
}

#[tokio::test]
async fn single_dml_uses_the_submission_terminal_row_count() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements("UPDATE items SET value = 1")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await
    .expect("single DML execution");

    assert_eq!(result.command_tag, Some(CommandTag::Update(3)));
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert_eq!(log.matches("__sabiql_marker").count(), 1);
    assert!(log.contains("ROW_COUNT()"));
}

#[tokio::test]
async fn tail_error_is_classified_after_pty_drain() {
    let (_directory, program, option_file) = fake_mysql_multi_with_tail_failure();
    let query = "UPDATE items SET value = 1; CREATE TABLE created (id INT)";
    let statements = split_mysql_statements(query)
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::QueryFailedAfterChange {
            source,
            refresh_scope: RefreshScope::Metadata,
            ..
        }) if matches!(&*source, DbOperationError::ObjectMissing(_))
    ));
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(log.lines().any(|line| line == "input_closed"), "{log}");
}

#[tokio::test]
async fn marker_failure_after_a_change_refreshes_the_current_scope() {
    let (_directory, program, option_file) = fake_mysql_multi_with_marker_failure();
    let statements = split_mysql_statements("UPDATE items SET value = 1")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::QueryFailedAfterChange {
            source,
            refresh_scope: RefreshScope::Data,
            ..
        }) if matches!(&*source, DbOperationError::QueryFailed(_))
    ));
}

#[tokio::test]
async fn first_change_statement_failure_refreshes_possible_scope() {
    for (details, summary) in [
        (
            "ERROR 1142 (42000): command denied to user",
            "Permission denied",
        ),
        (
            "ERROR 1062 (23000): Duplicate entry duplicate_value for key PRIMARY",
            "Unique constraint violation",
        ),
        (
            "ERROR 1452 (23000): Cannot add or update a child row: a foreign key constraint fails",
            "Foreign key constraint violation",
        ),
        (
            "ERROR 1205 (HY000): Lock wait timeout exceeded",
            "Operation blocked by lock or timeout",
        ),
    ] {
        let (_directory, program, option_file) = fake_mysql_multi_with_statement_failure(details);
        let statements = split_mysql_statements("UPDATE items SET value = 1")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;
        let Err(error) = result else {
            panic!("expected the fake MySQL statement to fail");
        };

        assert_eq!(error.summary(), summary);
        assert!(matches!(
            error,
            DbOperationError::QueryFailedAfterChange {
                source,
                refresh_scope: RefreshScope::Data,
            } if matches!(
                &*source,
                DbOperationError::PermissionDenied(_)
                    | DbOperationError::UniqueViolation(_)
                    | DbOperationError::ForeignKeyViolation(_)
                    | DbOperationError::LockTimeout(_)
            )
        ));
    }
}

#[tokio::test]
async fn marks_a_later_failure_after_a_confirmed_change_for_refresh() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements =
        split_mysql_statements("UPDATE items SET value = 1; SELECT missing_column FROM items")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::QueryFailedAfterChange {
            source,
            refresh_scope: RefreshScope::Data,
            ..
        }) if matches!(&*source, DbOperationError::ObjectMissing(_))
    ));
}

#[tokio::test]
async fn timeout_after_a_data_change_is_wrapped_for_refresh() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT SLEEP(40)")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_millis(100),
    )
    .await;

    assert!(matches!(
        result,
        Err(DbOperationError::QueryFailedAfterChange {
            source,
            refresh_scope: RefreshScope::Data,
            ..
        }) if matches!(&*source, DbOperationError::Timeout(_))
    ));
}

#[tokio::test]
async fn read_only_timeout_does_not_request_refresh() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements("SELECT SLEEP(40)")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadWrite,
        Duration::from_millis(100),
    )
    .await;

    assert!(matches!(result, Err(DbOperationError::Timeout(_))));
}

#[tokio::test]
async fn rejects_error_reported_without_a_statement_marker() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let statements = split_mysql_statements("SELECT missing_column FROM items")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
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
