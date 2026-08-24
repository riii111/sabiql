use super::*;

#[tokio::test]
async fn configures_read_only_session_before_user_sql() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let statements = split_mysql_statements("SELECT 2")
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadOnly,
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_else(|error| {
        let log = fs::read_to_string(&log_file).unwrap_or_default();
        panic!("read-only execution failed: {error:?}; log: {log}");
    });

    assert_eq!(
        result.result_set.unwrap().values[0][0].as_str(),
        Some("two")
    );
    let log = fs::read_to_string(log_file).unwrap();
    let session_index = log
        .find(MYSQL_READ_ONLY_STATEMENT)
        .expect("read-only session statement");
    let settings_index = log.find(MYSQL_SESSION_SETTINGS).expect("session settings");
    let mode_index = log.find("__sabiql_sql_mode").expect("sql_mode validation");
    let user_index = log.find("SELECT 2").expect("user statement");
    assert!(settings_index < session_index, "{log}");
    assert!(session_index < mode_index, "{log}");
    assert!(mode_index < user_index, "{log}");
    assert!(session_index < user_index, "{log}");
    assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
}

#[tokio::test]
async fn empty_result_uses_metadata_fallback_without_replaying_query() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let query = "SELECT 1 AS first_alias WHERE FALSE";
    let statements = split_mysql_statements(query)
        .unwrap()
        .into_iter()
        .map(|sql| classify_mysql_statement(&sql).unwrap())
        .collect::<Vec<_>>();

    let result = run_mysql_adhoc_with_program_and_statements(
        OsStr::new(&program),
        &option_file,
        &statements,
        AccessMode::ReadOnly,
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_else(|error| {
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
        panic!("empty result failed: {error:?}; log: {log}");
    });
    let result_set = result.result_set.expect("result set");
    assert_eq!(result_set.columns, ["first_alias"]);
    assert!(result_set.values.is_empty());

    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert_eq!(
        log.lines().filter(|line| *line == query).count(),
        1,
        "{log}"
    );
    assert!(log.contains("__sabiql_metadata_inner"));
}

#[tokio::test]
async fn duplicate_empty_select_columns_are_rejected_without_replaying_user_sql() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let query = "SELECT 1 AS duplicate_alias, 2 AS duplicate_alias WHERE FALSE";
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

    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(
        matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(ref details))
                if details.contains("duplicate column names")
        ),
        "result={result:?}; log={log}"
    );
    assert_eq!(
        log.lines().filter(|line| *line == query).count(),
        1,
        "{log}"
    );
}

#[tokio::test]
async fn generated_preview_and_metadata_queries_configure_read_only_session() {
    for query in [
        "SELECT id FROM app.items ORDER BY id LIMIT 10 OFFSET 0",
        "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES",
    ] {
        let (_directory, program, option_file) = fake_mysql_multi();
        let statements = split_mysql_statements(query)
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            &statements,
            AccessMode::ReadOnly,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
        let session_index = log
            .find(MYSQL_READ_ONLY_STATEMENT)
            .expect("read-only session statement");
        let query_index = log.find(query).expect("generated query");
        assert!(session_index < query_index, "{query}: {log}");
    }
}
