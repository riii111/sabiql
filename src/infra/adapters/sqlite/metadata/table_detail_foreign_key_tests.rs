use crate::adapters::test_support;
use crate::app::ports::outbound::MetadataProvider;
use crate::domain::UNRESOLVED_FK_COLUMN;

use super::SqliteAdapter;

#[tokio::test]
async fn composite_foreign_key_groups_columns_in_sequence_order() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
        CREATE TABLE child(
            x INTEGER,
            y INTEGER,
            FOREIGN KEY(x, y) REFERENCES parent(a, b)
        );
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(
        detail.foreign_keys[0].from_columns,
        vec!["x".to_string(), "y".to_string()]
    );
    assert_eq!(
        detail.foreign_keys[0].to_columns,
        vec!["a".to_string(), "b".to_string()]
    );
}

#[tokio::test]
async fn foreign_key_without_target_columns_resolves_parent_primary_key() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
        CREATE TABLE child(
            x INTEGER,
            y INTEGER,
            FOREIGN KEY(x, y) REFERENCES parent
        );
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(
        detail.foreign_keys[0].to_columns,
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(detail.foreign_keys[0].reference_resolved);
}

#[tokio::test]
async fn foreign_key_to_missing_table_is_kept_as_unresolved() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        PRAGMA foreign_keys=OFF;
        CREATE TABLE child(
            org_id INTEGER REFERENCES missing_orgs(id)
        );
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(detail.columns.len(), 1);
    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(detail.foreign_keys[0].to_table, "missing_orgs");
    assert_eq!(detail.foreign_keys[0].to_columns, vec!["id".to_string()]);
    assert!(!detail.foreign_keys[0].reference_resolved);
}

#[tokio::test]
async fn foreign_key_to_missing_column_is_kept_as_unresolved() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        PRAGMA foreign_keys=OFF;
        CREATE TABLE parent(a INTEGER PRIMARY KEY);
        CREATE TABLE child(
            x INTEGER REFERENCES parent(missing_col)
        );
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(
        detail.foreign_keys[0].to_columns,
        vec!["missing_col".to_string()]
    );
    assert!(!detail.foreign_keys[0].reference_resolved);
}

#[tokio::test]
async fn foreign_key_without_target_columns_and_missing_parent_pk_is_unresolved() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        PRAGMA foreign_keys=OFF;
        CREATE TABLE parent(a INTEGER);
        CREATE TABLE child(x INTEGER REFERENCES parent);
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(
        detail.foreign_keys[0].to_columns,
        vec![UNRESOLVED_FK_COLUMN.to_string()]
    );
    assert!(!detail.foreign_keys[0].reference_resolved);
}

#[tokio::test]
async fn foreign_key_target_column_matches_case_insensitively() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE parent(id INTEGER PRIMARY KEY);
        CREATE TABLE child(x INTEGER REFERENCES parent(ID));
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "child")
        .await
        .unwrap();

    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(detail.foreign_keys[0].to_columns, vec!["ID".to_string()]);
    assert!(detail.foreign_keys[0].reference_resolved);
}
