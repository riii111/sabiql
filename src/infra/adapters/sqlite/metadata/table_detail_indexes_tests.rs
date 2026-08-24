use crate::adapters::test_support;
use crate::app::ports::outbound::MetadataProvider;

use super::{RawIndexColumn, SqliteAdapter, index_key_column_names};

#[test]
fn index_key_column_names_preserves_expression_and_unknown_key_columns() {
    let columns = vec![
        RawIndexColumn {
            cid: 1,
            name: Some("email".to_string()),
            desc: 0,
            coll: None,
            key: 1,
        },
        RawIndexColumn {
            cid: -2,
            name: None,
            desc: 0,
            coll: None,
            key: 1,
        },
        RawIndexColumn {
            cid: 99,
            name: None,
            desc: 0,
            coll: None,
            key: 1,
        },
        RawIndexColumn {
            cid: 2,
            name: Some("rowid".to_string()),
            desc: 0,
            coll: None,
            key: 0,
        },
    ];

    assert_eq!(
        index_key_column_names(&columns),
        vec![
            "email".to_string(),
            "<expression>".to_string(),
            "<unknown>".to_string()
        ]
    );
}

#[tokio::test]
async fn partial_unique_index_does_not_mark_column_unique() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(email TEXT);
        CREATE UNIQUE INDEX idx_users_email_active
            ON users(email)
            WHERE email IS NOT NULL;
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();
    let email = detail
        .columns
        .iter()
        .find(|column| column.name == "email")
        .unwrap();
    assert!(!email.is_unique());
    let index = detail
        .indexes
        .iter()
        .find(|index| index.name == "idx_users_email_active")
        .unwrap();
    assert!(index.is_unique());
    assert!(index.is_partial());
    assert_eq!(index.columns, vec!["email".to_string()]);

    let light = adapter
        .fetch_table_columns_and_fks(&dsn, "main", "users")
        .await
        .unwrap();
    let light_email = light
        .columns
        .iter()
        .find(|column| column.name == "email")
        .unwrap();
    assert!(!light_email.is_unique());
    assert!(light.indexes.is_empty());
}

#[tokio::test]
async fn partial_expression_index_preserves_metadata_and_definition() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT);
        CREATE INDEX idx_users_email_lower
            ON users(lower(email))
            WHERE email IS NOT NULL;
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();
    let index = detail
        .indexes
        .iter()
        .find(|index| index.name == "idx_users_email_lower")
        .unwrap();

    assert_eq!(index.columns, vec!["<expression>".to_string()]);
    assert!(index.is_partial());
    assert!(index.has_expression());
    assert!(index.has_auxiliary_columns());
    assert!(index.needs_source_definition_detail());
    assert!(index.definition.as_deref().is_some_and(|definition| {
        definition.contains("lower(email)") && definition.contains("WHERE email IS NOT NULL")
    }));
}

#[tokio::test]
async fn partial_index_preserves_where_clause_in_definition() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(email TEXT);
        CREATE INDEX idx_users_email_active
            ON users(email)
            WHERE email IS NOT NULL;
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();
    let index = detail
        .indexes
        .iter()
        .find(|index| index.name == "idx_users_email_active")
        .unwrap();

    assert_eq!(index.columns, vec!["email".to_string()]);
    assert!(index.is_partial());
    assert!(index.needs_source_definition_detail());
    assert!(
        index
            .definition
            .as_deref()
            .is_some_and(|definition| { definition.contains("WHERE email IS NOT NULL") })
    );
}

#[tokio::test]
async fn descending_and_collation_indexes_preserve_definition() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(name TEXT, created_at TEXT);
        CREATE INDEX idx_users_name_desc ON users(name DESC);
        CREATE INDEX idx_users_name_nocase ON users(name COLLATE NOCASE);
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    let descending = detail
        .indexes
        .iter()
        .find(|index| index.name == "idx_users_name_desc")
        .unwrap();
    assert!(descending.has_descending_key());
    assert!(descending.needs_source_definition_detail());
    assert!(
        descending
            .definition
            .as_deref()
            .is_some_and(|definition| { definition.contains("DESC") })
    );

    let collation = detail
        .indexes
        .iter()
        .find(|index| index.name == "idx_users_name_nocase")
        .unwrap();
    assert!(collation.has_non_binary_collation());
    assert!(collation.needs_source_definition_detail());
    assert!(
        collation
            .definition
            .as_deref()
            .is_some_and(|definition| { definition.contains("COLLATE NOCASE") })
    );
}
