use super::*;

#[tokio::test]
async fn foreign_key_restrict_rejects_parent_delete_with_child_row() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE RESTRICT
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", AccessMode::ReadWrite)
        .await;
    let children = adapter
        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
        .await
        .unwrap();

    assert!(matches!(
        result,
        Err(DbOperationError::ForeignKeyViolation(_))
    ));
    assert_eq!(
        test_support::display_row(&children, 0),
        vec!["1".to_string()]
    );
}

#[tokio::test]
async fn unique_constraint_violation_is_classified() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        "CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE NOT NULL);",
    );
    let adapter = SqliteAdapter::new();

    adapter
        .execute_write(
            &dsn,
            "INSERT INTO users(id, email) VALUES (1, 'a@example.com')",
            AccessMode::ReadWrite,
        )
        .await
        .unwrap();

    let result = adapter
        .execute_write(
            &dsn,
            "INSERT INTO users(id, email) VALUES (2, 'a@example.com')",
            AccessMode::ReadWrite,
        )
        .await;

    assert!(matches!(result, Err(DbOperationError::UniqueViolation(_))));
}

#[tokio::test]
async fn syntax_error_stays_query_failed_with_details() {
    let (_dir, dsn) = test_support::make_sqlite_db("");
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_adhoc(&dsn, "SELEKT 1", AccessMode::ReadOnly)
        .await;

    assert!(matches!(result, Err(DbOperationError::QueryFailed(message))
                if message.to_ascii_lowercase().contains("syntax error")));
}

#[tokio::test]
async fn foreign_key_cascade_applies_to_parent_delete() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", AccessMode::ReadWrite)
        .await
        .unwrap();
    let children = adapter
        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
        .await
        .unwrap();

    assert_eq!(result.affected_rows, 1);
    assert_eq!(children.data_row_count(), 0);
}

#[tokio::test]
async fn returns_affected_rows() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
    );
    let adapter = SqliteAdapter::new();

    let result = adapter
        .execute_write(
            &dsn,
            "DELETE FROM users WHERE id IN (1, 2)",
            AccessMode::ReadWrite,
        )
        .await
        .unwrap();

    assert_eq!(result.affected_rows, 2);
}
