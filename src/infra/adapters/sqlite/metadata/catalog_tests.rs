use crate::app::ports::outbound::{
    DbOperationError, MetadataProvider, SQLITE_SAFE_MODE_REQUIRED_MARKER,
};
use crate::domain::{Schema, SqlitePathError, TableKind, TableKindInfo};

use super::super::super::sqlite3::metadata::RawTable;
use super::super::SqliteAdapter;
use super::kind_info_for_raw_table;

#[test]
fn legacy_list_row_uses_sql_for_storage() {
    let table: RawTable = serde_json::from_str(
        r#"{"name":"settings","sql":"CREATE TABLE settings(id INTEGER PRIMARY KEY) WITHOUT ROWID;"}"#,
    )
    .unwrap();

    let kind_info = kind_info_for_raw_table(&table);

    assert!(kind_info.without_rowid);
}

mod metadata {
    use crate::adapters::test_support;

    use super::*;

    #[tokio::test]
    async fn invalid_dsn_returns_connection_error() {
        let adapter = SqliteAdapter::new();

        let postgres_result = adapter.fetch_metadata("postgres://localhost").await;
        let empty_result = adapter.fetch_metadata("sqlite://").await;

        assert!(matches!(
            postgres_result,
            Err(DbOperationError::ConnectionFailed(_))
        ));
        assert!(matches!(
            empty_result,
            Err(DbOperationError::ConnectionFailed(_))
        ));
    }

    #[tokio::test]
    async fn rejects_sqlite_before_safe_mode_minimum_at_connection() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();
        let expects_rejection =
            std::env::var_os("SABIQL_EXPECT_SQLITE_SAFE_MODE_REJECTION").is_some();

        match adapter.fetch_metadata(&dsn).await {
            Err(DbOperationError::UnsupportedOperation(details)) if expects_rejection => {
                assert!(details.contains(SQLITE_SAFE_MODE_REQUIRED_MARKER));
            }
            Ok(_) if !expects_rejection => {}
            result => panic!("unexpected SQLite safe mode connection result: {result:?}"),
        }
    }

    #[tokio::test]
    async fn missing_database_file_returns_error_without_creating_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let dsn = format!("sqlite://{}", path.display());
        let adapter = SqliteAdapter::new();

        let result = adapter.fetch_metadata(&dsn).await;

        assert!(matches!(
            result,
            Err(DbOperationError::SqlitePath(SqlitePathError::FileNotFound(file_path)))
                if file_path == path.display().to_string()
        ));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn lists_user_tables_in_main_schema() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(id INTEGER PRIMARY KEY AUTOINCREMENT);
        ",
        );
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
        let table_names: Vec<_> = metadata
            .table_summaries
            .iter()
            .map(|summary| summary.name.as_str())
            .collect();

        assert_eq!(metadata.schemas, vec![Schema::new("main")]);
        assert_eq!(table_names, vec!["users"]);
        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
        assert!(metadata.table_summaries[0].row_count_estimate.is_none());
    }

    #[tokio::test]
    async fn skips_row_count_even_when_table_has_rows() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        INSERT INTO users(id) VALUES (1), (2), (3);
        ",
        );
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.table_summaries.len(), 1);
        assert!(metadata.table_summaries[0].row_count_estimate.is_none());
    }

    #[tokio::test]
    async fn empty_database_returns_no_tables() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.schemas, vec![Schema::new("main")]);
        assert!(metadata.table_summaries.is_empty());
    }

    #[tokio::test]
    async fn hides_fts5_shadow_tables_from_normal_table_list() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);
        CREATE VIRTUAL TABLE notes_fts USING fts5(body);
        ",
        );
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
        let table_names: Vec<_> = metadata
            .table_summaries
            .iter()
            .map(|summary| summary.name.as_str())
            .collect();

        assert_eq!(table_names, vec!["notes", "notes_fts"]);
    }

    struct TableKindInfoMetadataFixture {
        _dir: tempfile::TempDir,
        kind_info_by_name: std::collections::HashMap<String, TableKindInfo>,
    }

    impl TableKindInfoMetadataFixture {
        async fn new() -> Self {
            let (dir, dsn) = test_support::make_sqlite_db(
                r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        CREATE TABLE strict_users(id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE settings(
            key TEXT PRIMARY KEY,
            value TEXT
        ) WITHOUT ROWID;
        CREATE TABLE typed_users(id INTEGER PRIMARY KEY, name TEXT) STRICT;
        CREATE VIRTUAL TABLE notes_fts USING fts5(body);
        ",
            );
            let adapter = SqliteAdapter::new();
            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
            let kind_info_by_name = metadata
                .table_summaries
                .iter()
                .map(|summary| (summary.name.clone(), summary.kind_info.clone()))
                .collect();

            Self {
                _dir: dir,
                kind_info_by_name,
            }
        }

        fn kind_info(&self, name: &str) -> &TableKindInfo {
            &self.kind_info_by_name[name]
        }
    }

    #[tokio::test]
    async fn classifies_regular_table_kind() {
        let fixture = TableKindInfoMetadataFixture::new().await;

        assert_eq!(fixture.kind_info("users").kind, TableKind::Table);
        assert!(!fixture.kind_info("users").is_strict);
        assert!(!fixture.kind_info("users").without_rowid);
    }

    #[tokio::test]
    async fn classifies_without_rowid_table_kind() {
        let fixture = TableKindInfoMetadataFixture::new().await;

        assert!(fixture.kind_info("settings").without_rowid);
    }

    #[tokio::test]
    async fn does_not_infer_strict_from_table_name() {
        let fixture = TableKindInfoMetadataFixture::new().await;

        assert!(
            !fixture.kind_info("strict_users").is_strict,
            "table name containing 'strict' must not infer STRICT from DDL when pragma.strict is 0"
        );
    }

    #[tokio::test]
    async fn classifies_strict_table_kind() {
        let fixture = TableKindInfoMetadataFixture::new().await;

        assert!(fixture.kind_info("typed_users").is_strict);
    }

    #[tokio::test]
    async fn classifies_virtual_table_kind() {
        let fixture = TableKindInfoMetadataFixture::new().await;

        assert_eq!(fixture.kind_info("notes_fts").kind, TableKind::Virtual);
        assert_eq!(
            fixture.kind_info("notes_fts").virtual_module.as_deref(),
            Some("fts5")
        );
    }

    #[tokio::test]
    async fn hides_rtree_shadow_tables_from_normal_table_list() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE places(id INTEGER PRIMARY KEY, name TEXT);
        CREATE VIRTUAL TABLE places_geo USING rtree(
            id,
            minX, maxX,
            minY, maxY
        );
        ",
        );
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
        let table_names: Vec<_> = metadata
            .table_summaries
            .iter()
            .map(|summary| summary.name.as_str())
            .collect();

        assert_eq!(table_names, vec!["places", "places_geo"]);
    }
}
