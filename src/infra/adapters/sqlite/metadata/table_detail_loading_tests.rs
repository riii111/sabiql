use crate::adapters::test_support;
use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{FkAction, IndexType, TableKind};

use super::SqliteAdapter;

#[tokio::test]
async fn inspector_metadata_uses_one_sqlite_process() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE organizations(id INTEGER PRIMARY KEY, name TEXT);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                email TEXT UNIQUE,
                org_id INTEGER REFERENCES organizations(id)
            );
            CREATE INDEX idx_users_org_id ON users(org_id DESC);
            CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END;
            ",
    );
    let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    assert_eq!(detail.indexes.len(), 2);
    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(detail.triggers.len(), 1);
    assert_eq!(process_counter.count(), 1);
}

#[tokio::test]
async fn inspector_returns_metadata_when_view_row_count_fails() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE VIEW broken_json AS
            SELECT * FROM json_each('invalid');
            ",
    );
    let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "broken_json")
        .await
        .unwrap();

    assert_eq!(detail.kind_info.kind, TableKind::View);
    assert!(!detail.columns.is_empty());
    assert!(detail.row_count_estimate.is_none());
    assert_eq!(process_counter.count(), 2);
}

#[tokio::test]
async fn completion_metadata_uses_one_sqlite_process() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
            CREATE TABLE organizations(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                email TEXT UNIQUE,
                org_id INTEGER REFERENCES organizations(id)
            );
            CREATE INDEX idx_users_org_id ON users(org_id);
            ",
    );
    let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

    let detail = adapter
        .fetch_table_columns_and_fks(&dsn, "main", "users")
        .await
        .unwrap();

    assert!(detail.columns[1].is_unique());
    assert_eq!(detail.foreign_keys.len(), 1);
    assert_eq!(process_counter.count(), 1);
}

#[tokio::test]
async fn non_main_schema_returns_object_missing() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
    let adapter = SqliteAdapter::new();

    let result = adapter.fetch_table_detail(&dsn, "other", "users").await;

    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
}

#[tokio::test]
async fn missing_table_returns_object_missing() {
    let (_dir, dsn) = test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
    let adapter = SqliteAdapter::new();

    let result = adapter.fetch_table_detail(&dsn, "main", "missing").await;

    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
}

#[tokio::test]
async fn resolves_table_name_case_insensitively() {
    let (_dir, dsn) =
        test_support::make_sqlite_db("CREATE TABLE MixedCase(id INTEGER PRIMARY KEY);");
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "mixedcase")
        .await
        .unwrap();

    assert_eq!(detail.primary_key, Some(vec!["id".to_string()]));
    assert_eq!(detail.kind_info.kind, TableKind::Table);
}

#[tokio::test]
async fn loads_columns_indexes_and_foreign_keys() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE orgs(id INTEGER PRIMARY KEY);
        CREATE TABLE users(
            id INTEGER PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_users_org_id ON users(org_id);
        INSERT INTO orgs(id) VALUES (1);
        INSERT INTO users(id, email, org_id) VALUES (1, 'a@example.com', 1);
        ",
    );
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    assert_eq!(detail.primary_key, Some(vec!["id".to_string()]));
    assert_eq!(detail.row_count_estimate, Some(1));
    assert!(
        detail.columns.iter().any(|column| {
            column.name == "email" && !column.is_nullable() && column.is_unique()
        })
    );
    assert!(
        detail
            .indexes
            .iter()
            .any(|index| index.name == "idx_users_org_id"
                && index.columns == vec!["org_id".to_string()]
                && index.index_type == IndexType::Unknown)
    );
    let fk = detail
        .foreign_keys
        .iter()
        .find(|fk| fk.to_table == "orgs")
        .unwrap();
    assert_eq!(fk.from_columns, vec!["org_id".to_string()]);
    assert_eq!(fk.to_columns, vec!["id".to_string()]);
    assert_eq!(fk.on_delete, FkAction::Cascade);
    assert!(detail.rls.is_none());
    assert!(detail.triggers.is_empty());
}

#[tokio::test]
async fn columns_and_fks_skips_row_count() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        INSERT INTO users(id) VALUES (1), (2), (3);
        ",
    );
    let adapter = SqliteAdapter::new();

    let light = adapter
        .fetch_table_columns_and_fks(&dsn, "main", "users")
        .await
        .unwrap();
    let full = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    assert!(light.row_count_estimate.is_none());
    assert_eq!(full.row_count_estimate, Some(3));
}

#[tokio::test]
async fn columns_and_fks_skips_triggers_and_source_ddl() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN
            SELECT 1;
        END;
        ",
    );
    let adapter = SqliteAdapter::new();

    let light = adapter
        .fetch_table_columns_and_fks(&dsn, "main", "users")
        .await
        .unwrap();
    let full = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    assert!(light.triggers.is_empty());
    assert!(light.source_ddl().is_none());
    assert_eq!(full.triggers.len(), 1);
    assert!(full.source_ddl().is_some());
}
