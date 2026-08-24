use super::*;

#[tokio::test]
async fn count_query_rows_parses_count_result() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
    );
    let adapter = SqliteAdapter::new();

    let count = adapter
        .count_query_rows(&dsn, "SELECT COUNT(*) FROM users")
        .await
        .unwrap();

    assert_eq!(count, 3);
}

#[tokio::test]
async fn count_query_rows_rejects_fsdir_before_execution() {
    let (_dir, dsn) = test_support::make_sqlite_db("");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .count_query_rows(
            &dsn,
            "SELECT COUNT(*) FROM (SELECT * FROM fsdir('/tmp')) AS files",
        )
        .await;

    assert!(matches!(
        result,
        Err(DbOperationError::UnsupportedOperation(details))
            if details == "SQLite fsdir access is not supported in safe mode"
    ));
}

#[tokio::test]
async fn export_to_csv_writes_rows() {
    let (dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
    );
    let path = dir.path().join("users.csv");
    let adapter = SqliteAdapter::new();

    adapter
        .cli
        .export_csv(
            SqliteAdapter::path_from_dsn(&dsn).unwrap(),
            "SELECT id, name FROM users ORDER BY id",
            &path,
            true,
        )
        .await
        .unwrap();
    let csv = std::fs::read_to_string(path).unwrap();

    assert_eq!(csv, "id,name\n1,a\n2,b\n");
}

#[tokio::test]
async fn export_to_csv_runs_sql_larger_than_the_windows_command_line_limit() {
    let (dir, dsn) = test_support::make_sqlite_db("");
    let path = dir.path().join("long.csv");
    let adapter = SqliteAdapter::new();
    let sql = format!("SELECT 1 AS value /* {} */", "x".repeat(32_768));

    adapter
        .cli
        .export_csv(
            SqliteAdapter::path_from_dsn(&dsn).unwrap(),
            &sql,
            &path,
            true,
        )
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), "value\n1\n");
}

#[tokio::test]
async fn export_to_csv_preserves_records_with_embedded_newlines() {
    let (dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE logs(id INTEGER PRIMARY KEY, message TEXT);
            INSERT INTO logs(id, message) VALUES (1, 'hello
world'), (2, 'done');
            ",
    );
    let path = dir.path().join("logs.csv");
    let adapter = SqliteAdapter::new();

    adapter
        .cli
        .export_csv(
            SqliteAdapter::path_from_dsn(&dsn).unwrap(),
            "SELECT id, message FROM logs ORDER BY id",
            &path,
            true,
        )
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "id,message\n1,\"hello\nworld\"\n2,done\n"
    );
}

#[tokio::test]
async fn export_to_csv_rejects_write_sql() {
    let (dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
    let path = dir.path().join("write_export.csv");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .export_to_csv(&dsn, "INSERT INTO users(id) VALUES (1)", "write_export")
        .await;

    assert!(matches!(
        result,
        Err(DbOperationError::UnsupportedOperation(message))
        if message.contains("write or DDL")
    ));
    assert!(!path.exists());
}

#[tokio::test]
async fn export_to_csv_rejects_fsdir_before_creating_output() {
    let (_dir, dsn) = test_support::make_sqlite_db("");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .export_to_csv(&dsn, "SELECT * FROM fsdir('/tmp')", "fsdir_export")
        .await;

    assert!(matches!(
        result,
        Err(DbOperationError::UnsupportedOperation(details))
            if details == "SQLite fsdir access is not supported in safe mode"
    ));
}

#[tokio::test]
async fn export_to_csv_missing_table_returns_object_missing_and_removes_file() {
    let (dir, dsn) = test_support::make_sqlite_db("");
    let path = dir.path().join("missing_export.csv");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .export_to_csv(&dsn, "SELECT id FROM missing", "missing_export")
        .await;

    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    assert!(!path.exists());
}

#[tokio::test]
async fn count_query_rows_missing_table_returns_object_missing() {
    let (_dir, dsn) = test_support::make_sqlite_db("");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .count_query_rows(&dsn, "SELECT COUNT(*) FROM missing")
        .await;

    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
}

#[tokio::test]
async fn read_only_write_fails() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_write(
            &dsn,
            "INSERT INTO users(id) VALUES (1)",
            AccessMode::ReadOnly,
        )
        .await;

    assert!(matches!(result, Err(DbOperationError::PermissionDenied(_))));
}

#[tokio::test]
async fn missing_database_is_rejected_without_creating_an_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.db");
    let dsn = format!("sqlite://{}", path.display());
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_write(
            &dsn,
            "CREATE TABLE users(id INTEGER)",
            AccessMode::ReadWrite,
        )
        .await;

    assert!(matches!(
        result,
        Err(DbOperationError::ConnectionFailed(details))
            if details.contains("SQLite database file not found")
    ));
    assert!(!path.exists());
}
