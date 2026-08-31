use crate::app::ports::outbound::{DbOperationError, MetadataProvider, SqliteCompatibilityKind};
use crate::domain::{Schema, SqlitePathError, TableKind, TableKindInfo};

use super::super::SqliteAdapter;

mod metadata {
    use crate::adapters::test_support;
    use rstest::rstest;

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
            Err(DbOperationError::UnsupportedOperationWithSqliteKind {
                kind: SqliteCompatibilityKind::SafeMode,
                details,
            }) if expects_rejection => {
                assert!(details.contains("3.41.1"));
                assert!(!details.contains("SQLITE_SAFE_MODE_REQUIRED"));
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
    async fn lists_user_tables_in_main_schema_with_one_process() {
        if let Ok(dsn) = std::env::var("SABIQL_PROCESS_COUNTER_DSN") {
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
            return;
        }

        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(id INTEGER PRIMARY KEY AUTOINCREMENT);
        ",
        );

        #[cfg(unix)]
        {
            assert_catalog_metadata_uses_one_sqlite_process(&dsn);
            return;
        }

        #[cfg(not(unix))]
        let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

        #[cfg(not(unix))]
        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        #[cfg(not(unix))]
        {
            let table_names: Vec<_> = metadata
                .table_summaries
                .iter()
                .map(|summary| summary.name.as_str())
                .collect();

            assert_eq!(metadata.schemas, vec![Schema::new("main")]);
            assert_eq!(table_names, vec!["users"]);
            assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
            assert!(metadata.table_summaries[0].row_count_estimate.is_none());
            assert_eq!(process_counter.count(), 1);
        }
    }

    #[cfg(unix)]
    fn assert_catalog_metadata_uses_one_sqlite_process(dsn: &str) {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let wrapper_dir = tempfile::tempdir().unwrap();
        let counter_file = wrapper_dir.path().join("process-count");
        let wrapper = wrapper_dir.path().join("sqlite3");
        let real_sqlite3 = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("sqlite3"))
            .find(|path| path.is_file())
            .expect("sqlite3 must be available on PATH");
        fs::write(
            &wrapper,
            "#!/bin/sh\nprintf '%s\\n' 1 >> \"$SABIQL_PROCESS_COUNTER_FILE\"\nexec \"$SABIQL_REAL_SQLITE3\" \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();

        let path = std::env::var_os("PATH").unwrap();
        let child_path = std::env::join_paths(
            std::iter::once(wrapper_dir.path().to_path_buf()).chain(std::env::split_paths(&path)),
        )
        .unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .arg("adapters::sqlite::metadata::catalog::tests::metadata::lists_user_tables_in_main_schema_with_one_process")
            .arg("--exact")
            .arg("--nocapture")
            .env("SABIQL_PROCESS_COUNTER_FILE", &counter_file)
            .env("SABIQL_REAL_SQLITE3", real_sqlite3)
            .env("SABIQL_PROCESS_COUNTER_DSN", dsn)
            .env("PATH", child_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child metadata test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let starts = fs::read_to_string(&counter_file)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(starts, 1, "metadata should start sqlite3 exactly once");
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

    #[rstest]
    #[case::regular("users", TableKind::Table, false, false, None)]
    #[case::without_rowid("settings", TableKind::Table, false, true, None)]
    #[case::name_containing_strict("strict_users", TableKind::Table, false, false, None)]
    #[case::strict("typed_users", TableKind::Table, true, false, None)]
    #[case::virtual_table("notes_fts", TableKind::Virtual, false, false, Some("fts5"))]
    #[tokio::test]
    async fn classifies_table_kind(
        #[case] table_name: &str,
        #[case] expected_kind: TableKind,
        #[case] expected_strict: bool,
        #[case] expected_without_rowid: bool,
        #[case] expected_virtual_module: Option<&str>,
    ) {
        let fixture = TableKindInfoMetadataFixture::new().await;
        let kind_info = fixture.kind_info(table_name);

        assert_eq!(kind_info.kind, expected_kind);
        assert_eq!(kind_info.is_strict, expected_strict);
        assert_eq!(kind_info.without_rowid, expected_without_rowid);
        assert_eq!(kind_info.virtual_module.as_deref(), expected_virtual_module);
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
