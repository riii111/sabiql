use crate::adapters::mysql::option_file::MySqlOptionFile;

use super::*;

#[tokio::test]
async fn reuses_one_process_for_ordered_resultsets() {
    let (_directory, program, option_file_path) = fake_mysql_multi();
    let option_file = MySqlOptionFile {
        path: option_file_path.clone(),
    };
    let mut session =
        MySqlMetadataSession::spawn_with_metadata_program(OsStr::new(&program), option_file)
            .expect("spawn fake mysql");
    assert!(option_file_path.exists());

    session
        .prepare_read_only_and_probe()
        .await
        .expect("read-only session setup and metadata probe");
    for query in [
        "SELECT TABLES",
        "SELECT COLUMNS",
        "SELECT INDEXES",
        "SELECT FOREIGN_KEYS",
        "SELECT TRIGGERS",
        "SHOW CREATE TABLE items",
    ] {
        session.execute(query).await.expect("metadata resultset");
    }
    let empty_result = session
        .execute_with_expected_columns("EMPTY_RESULT", &["known_column"])
        .await
        .expect("empty metadata resultset");
    assert_eq!(empty_result.columns, ["known_column"]);
    assert!(empty_result.values.is_empty());
    session.finish().await.expect("finish fake mysql");
    drop(session);
    assert!(!option_file_path.exists());

    let log = fs::read_to_string(format!("{}.log", option_file_path.display())).unwrap();
    assert_eq!(
        log.lines()
            .filter(|line| line.starts_with("process="))
            .count(),
        1
    );
    let argv = log.lines().find(|line| line.starts_with("argv=")).unwrap();
    assert!(!argv.contains("--quick"), "{argv}");
    assert!(!argv.contains("--max-allowed-packet="), "{argv}");
    let positions = [
        MYSQL_SESSION_SETTINGS,
        MYSQL_READ_ONLY_STATEMENT,
        MYSQL_SESSION_MARKER_COLUMN,
        "__sabiql_sql_mode",
        "__sabiql_probe",
        "SELECT TABLES",
        "SELECT COLUMNS",
        "SELECT INDEXES",
        "SELECT FOREIGN_KEYS",
        "SELECT TRIGGERS",
        "SHOW CREATE TABLE items",
    ]
    .into_iter()
    .map(|query| log.find(query).expect("query in transcript"))
    .collect::<Vec<_>>();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
}

#[tokio::test]
async fn external_metadata_fallback_configures_read_only_session_before_query() {
    let (_directory, program, option_file) = fake_mysql_metadata_columns(false);
    let columns = mysql_metadata_columns_external_with_program(
        OsStr::new(&program),
        &option_file,
        "SHOW DATABASES",
        AccessMode::ReadOnly,
    )
    .await
    .unwrap_or_else(|error| {
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
        panic!("external metadata fallback failed: {error:?}; log: {log}");
    });

    assert_eq!(columns, ["Database"]);
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    let positions = [
        MYSQL_SESSION_SETTINGS,
        MYSQL_READ_ONLY_STATEMENT,
        MYSQL_SESSION_MARKER_COLUMN,
        "__sabiql_sql_mode",
        "SHOW DATABASES",
    ]
    .into_iter()
    .map(|query| log.find(query).expect("query in transcript"))
    .collect::<Vec<_>>();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
}

#[tokio::test]
async fn external_metadata_fallback_setup_failure_never_sends_query() {
    let (_directory, program, option_file) = fake_mysql_metadata_columns(true);
    let result = mysql_metadata_columns_external_with_program(
        OsStr::new(&program),
        &option_file,
        "SHOW DATABASES",
        AccessMode::ReadOnly,
    )
    .await;

    assert!(result.is_err());
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
    assert!(!log.contains("SHOW DATABASES"), "{log}");
}

#[tokio::test]
async fn external_metadata_timeout_kills_and_reaps_the_process() {
    let (_directory, program, option_file) =
        fake_mysql_metadata_columns_with_hanging_query(false, true);
    let result = run_mysql_metadata_query_with_read_only_session_with_timeout(
        OsStr::new(&program),
        &option_file,
        "SHOW DATABASES",
        Duration::from_secs(10),
    )
    .await;

    assert!(matches!(result, Err(DbOperationError::Timeout(_))));
    let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
    let pid = log
        .lines()
        .find_map(|line| line.strip_prefix("process=")?.parse::<i32>().ok())
        .expect("metadata process pid");
    let status = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("check metadata process");
    assert!(!status.success(), "metadata process {pid} is still running");
}
