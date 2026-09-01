use super::*;

#[tokio::test]
async fn exports_mysql_xml_rows_through_the_shared_csv_writer() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let path = option_file.with_file_name("export.csv");

    export_mysql_csv_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 1",
        path.clone(),
        Duration::from_secs(5),
        MySqlServerCapabilities::default(),
    )
    .await
    .unwrap();

    assert_eq!(fs::read_to_string(path).unwrap(), "value\none\n");
}

#[tokio::test]
async fn configures_read_only_session_before_user_sql() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let path = option_file.with_file_name("export.csv");

    export_mysql_csv_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 1",
        path,
        Duration::from_secs(5),
        MySqlServerCapabilities::default(),
    )
    .await
    .unwrap();

    let log = fs::read_to_string(log_file).unwrap();
    let session_index = log
        .find(MYSQL_READ_ONLY_STATEMENT)
        .expect("read-only session statement");
    let user_index = log.find("SELECT 1").expect("user statement");
    assert!(session_index < user_index, "{log}");
    assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
}

#[tokio::test]
async fn ignores_cli_error_text_inside_resultset_fields() {
    let (_directory, program, option_file) = fake_mysql("field_error");
    let path = option_file.with_file_name("export.csv");

    export_mysql_csv_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 1",
        path.clone(),
        Duration::from_secs(5),
        MySqlServerCapabilities::default(),
    )
    .await
    .unwrap();

    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "message\n\"line 1\nERROR 1146 (42S02): this is a cell value\"\n"
    );
}

#[tokio::test]
async fn read_only_session_failure_never_writes_user_sql_or_partial_file() {
    let (_directory, program, option_file) = fake_mysql("read_only_failure");
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let output_directory = tempfile::tempdir().unwrap();
    let final_path = output_directory.path().join("export.csv");

    let result = export_to_path(final_path.clone(), |path| {
        export_mysql_csv_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            path,
            Duration::from_secs(5),
            MySqlServerCapabilities::default(),
        )
    })
    .await;

    assert!(result.is_err());
    let log = fs::read_to_string(log_file).unwrap();
    assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
    assert!(!log.contains("SELECT 123"), "{log}");
    assert!(!final_path.exists());
    assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
}

#[tokio::test]
async fn failure_removes_the_partial_file() {
    let (_directory, program, option_file) = fake_mysql("failure");
    let output_directory = tempfile::tempdir().unwrap();
    let final_path = output_directory.path().join("export.csv");

    let result = export_to_path(final_path.clone(), |path| {
        export_mysql_csv_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 1",
            path,
            Duration::from_secs(5),
            MySqlServerCapabilities::default(),
        )
    })
    .await;

    assert!(result.is_err());
    assert!(!final_path.exists());
    assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
}

#[tokio::test]
async fn timeout_kills_the_process_and_removes_the_partial_file() {
    let (_directory, program, option_file) = fake_mysql("timeout");
    let output_directory = tempfile::tempdir().unwrap();
    let final_path = output_directory.path().join("export.csv");

    let result = export_to_path(final_path.clone(), |path| {
        export_mysql_csv_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 1",
            path,
            Duration::from_millis(50),
            MySqlServerCapabilities::default(),
        )
    })
    .await;

    assert!(matches!(result, Err(DbOperationError::Timeout(_))));
    assert!(!final_path.exists());
    assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
}

#[tokio::test]
async fn exports_empty_mysql_57_select_with_derived_metadata_fallback() {
    let (_directory, program, option_file) = fake_mysql_multi();
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let path = option_file.with_file_name("export.csv");

    export_mysql_csv_with_program(
        OsStr::new(&program),
        &option_file,
        "SELECT 1 WHERE FALSE",
        path.clone(),
        Duration::from_secs(5),
        MySqlServerCapabilities::from_version("5.7.44", 0),
    )
    .await
    .unwrap();

    assert_eq!(fs::read_to_string(path).unwrap(), "value\n");
    let log = fs::read_to_string(log_file).unwrap();
    assert!(log.contains("SELECT __sabiql_metadata_source_"), "{log}");
    assert!(!log.contains("WITH __sabiql_metadata_source_"), "{log}");
}
