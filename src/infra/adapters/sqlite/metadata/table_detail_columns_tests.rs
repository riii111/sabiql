use crate::adapters::test_support;
use crate::app::policy::column::column_read_only_reason;
use crate::app::ports::outbound::MetadataProvider;

use super::SqliteAdapter;

#[tokio::test]
async fn without_primary_key_sets_primary_key_none() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE logs(message TEXT);");
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "logs")
        .await
        .unwrap();

    assert_eq!(detail.primary_key, None);
    assert_eq!(detail.columns.len(), 1);
}

#[tokio::test]
async fn primary_key_nullability_matches_sqlite_metadata() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE regular(key TEXT PRIMARY KEY, value TEXT);
        CREATE TABLE without_rowid(key TEXT PRIMARY KEY, value TEXT) WITHOUT ROWID;
        ",
    );
    let adapter = SqliteAdapter::new();

    let regular = adapter
        .fetch_table_detail(&dsn, "main", "regular")
        .await
        .unwrap();
    let without_rowid = adapter
        .fetch_table_detail(&dsn, "main", "without_rowid")
        .await
        .unwrap();

    let regular_key = regular
        .columns
        .iter()
        .find(|column| column.name == "key")
        .unwrap();
    let without_rowid_key = without_rowid
        .columns
        .iter()
        .find(|column| column.name == "key")
        .unwrap();

    assert!(regular_key.is_primary_key());
    assert!(regular_key.is_nullable());
    assert!(without_rowid_key.is_primary_key());
    assert!(!without_rowid_key.is_nullable());
}

#[tokio::test]
async fn columns_and_fks_preserves_unique_column_attributes_without_returning_indexes() {
    let (_dir, dsn) =
        test_support::make_sqlite_db("CREATE TABLE users(email TEXT UNIQUE NOT NULL);");
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_columns_and_fks(&dsn, "main", "users")
        .await
        .unwrap();

    assert!(detail.indexes.is_empty());
    assert!(
        detail
            .columns
            .iter()
            .any(|column| column.name == "email" && column.is_unique())
    );
}

#[tokio::test]
async fn generated_and_hidden_columns_are_read_only() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(
            id INTEGER PRIMARY KEY,
            name TEXT,
            name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
        );
        CREATE VIRTUAL TABLE notes_fts USING fts5(body);
        ",
    );
    let adapter = SqliteAdapter::new();

    let users = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();
    let generated = users
        .columns
        .iter()
        .find(|column| column.name == "name_upper")
        .unwrap();
    assert!(generated.is_read_only());
    assert!(generated.is_generated());
    assert_eq!(column_read_only_reason(generated), Some("generated"));

    let fts = adapter
        .fetch_table_detail(&dsn, "main", "notes_fts")
        .await
        .unwrap();
    let hidden = fts
        .columns
        .iter()
        .find(|column| column.name == "notes_fts")
        .unwrap();
    assert!(hidden.is_read_only());
    assert!(hidden.is_hidden());
    assert_eq!(column_read_only_reason(hidden), Some("hidden"));
}
