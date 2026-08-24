use super::*;

#[tokio::test]
async fn metadata_and_rows_use_at_most_two_sqlite_processes() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
                CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE, org_id INTEGER);
                CREATE INDEX idx_users_org_id ON users(org_id);
                INSERT INTO users(id, email, org_id) VALUES (1, 'a@example.com', 7);
                ",
    );
    let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

    adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(process_counter.count(), 2);
}

#[tokio::test]
async fn returns_columns_rows_and_respects_pagination() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (2, 'b'), (1, 'a'), (3, 'c');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "users", 1, 1)
        .await
        .unwrap();

    assert_eq!(result.source, QuerySource::Preview);
    assert_eq!(result.columns, vec!["id", "name"]);
    assert_eq!(
        test_support::display_row(&result, 0),
        vec!["2".to_string(), "b".to_string()]
    );
}

#[tokio::test]
async fn primary_keyless_preview_exposes_only_user_columns() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE logs(rowid TEXT, message TEXT);
            INSERT INTO logs(rowid, message) VALUES ('user-visible', 'first');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "logs", 10, 0)
        .await
        .unwrap();

    assert_eq!(result.columns, vec!["rowid", "message"]);
    assert_eq!(
        test_support::display_row(&result, 0),
        vec!["user-visible".to_string(), "first".to_string()]
    );
}

#[tokio::test]
async fn rejects_non_main_schema() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
    let adapter = SqliteAdapter::new();

    let result = adapter.execute_preview(&dsn, "other", "users", 10, 0).await;

    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
}

#[tokio::test]
async fn preserves_nul_text_primary_key_for_preview_and_delete() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id TEXT PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES ('a' || char(0) || 'bc', 'target'), ('only', 'other');
            ",
    );
    let adapter = SqliteAdapter::new();

    let preview = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(
        preview.value_at(0, 0),
        Some(&QueryValue::Text("a\0bc".to_string()))
    );
    assert_eq!(preview.display_value_at(0, 0).as_deref(), Some("a\\0bc"));

    let delete_sql = adapter.build_bulk_delete_sql(
        DatabaseType::SQLite,
        "main",
        "users",
        &[vec![(
            "id".to_string(),
            QueryValue::Text("a\0bc".to_string()),
        )]],
    );
    let write = adapter
        .execute_write(&dsn, &delete_sql, AccessMode::ReadWrite)
        .await
        .unwrap();
    assert_eq!(write.affected_rows, 1);

    let remaining = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();
    assert_eq!(remaining.row_count(), 1);
    assert_eq!(
        remaining.value_at(0, 0),
        Some(&QueryValue::Text("only".to_string()))
    );
}

#[tokio::test]
async fn excludes_hidden_columns_from_preview_select_list() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            INSERT INTO notes_fts(body) VALUES ('hello');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "notes_fts", 10, 0)
        .await
        .unwrap();

    assert_eq!(result.columns, vec!["body"]);
    assert_eq!(
        result.value_at(0, 0),
        Some(&QueryValue::Text("hello".to_string()))
    );
}

#[tokio::test]
async fn empty_rowid_table_preview_keeps_all_visible_columns() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(name TEXT, email TEXT);");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(result.columns, vec!["name", "email"]);
    assert_eq!(result.data_row_count(), 0);
    assert_eq!(result.row_count(), 0);
}

#[tokio::test]
async fn preserves_distinct_c0_text_values_in_preview() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, value TEXT);
            INSERT INTO users(value) VALUES (char(1) || char(1));
            INSERT INTO users(value) VALUES (char(1) || char(92) || 'u0001');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(
        result.value_at(0, 1),
        Some(&QueryValue::Text("\x01\x01".to_string()))
    );
    assert_eq!(
        result.value_at(1, 1),
        Some(&QueryValue::Text("\x01\\u0001".to_string()))
    );
}

#[tokio::test]
async fn preserves_sentinel_like_text_without_nul_in_preview() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, token TEXT);
            INSERT INTO users(token) VALUES (char(1) || 'SABIQL_HEX:4142');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(
        result.value_at(0, 1),
        Some(&QueryValue::Text(format!(
            "{}4142",
            sql::sqlite_nul_text_sentinel()
        )))
    );
}

#[tokio::test]
async fn keeps_generated_columns_in_preview_select_list() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                name TEXT,
                name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
            );
            INSERT INTO users(name) VALUES ('alice');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_preview(&dsn, "main", "users", 10, 0)
        .await
        .unwrap();

    assert_eq!(result.columns, vec!["id", "name", "name_upper"]);
    assert_eq!(
        result.value_at(0, 2),
        Some(&QueryValue::Text("ALICE".to_string()))
    );
}
