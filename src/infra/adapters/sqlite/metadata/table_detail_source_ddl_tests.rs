use crate::adapters::test_support;
use crate::app::ports::outbound::{DdlGenerator, MetadataProvider};
use crate::domain::{DatabaseType, TableKind};

use super::SqliteAdapter;

#[tokio::test]
async fn source_ddl_preserves_without_rowid_and_virtual_table_syntax() {
    let (_dir, dsn) = test_support::make_sqlite_db(
        r"
        CREATE TABLE settings(
            key TEXT PRIMARY KEY,
            value TEXT
        ) WITHOUT ROWID;
        CREATE VIRTUAL TABLE notes_fts USING fts5(body);
        CREATE VIEW settings_view AS SELECT key, value FROM settings;
        ",
    );
    let adapter = SqliteAdapter::new();

    let without_rowid = adapter
        .fetch_table_detail(&dsn, "main", "settings")
        .await
        .unwrap();
    assert!(
        without_rowid
            .source_ddl()
            .is_some_and(|ddl| ddl.contains("WITHOUT ROWID"))
    );
    assert!(without_rowid.kind_info.without_rowid);
    assert_eq!(
        adapter.generate_ddl(DatabaseType::SQLite, &without_rowid),
        without_rowid.source_ddl().unwrap()
    );

    let virtual_table = adapter
        .fetch_table_detail(&dsn, "main", "notes_fts")
        .await
        .unwrap();
    assert!(
        virtual_table
            .source_ddl()
            .is_some_and(|ddl| ddl.starts_with("CREATE VIRTUAL TABLE"))
    );
    assert_eq!(virtual_table.kind_info.kind, TableKind::Virtual);
    assert_eq!(
        virtual_table.kind_info.virtual_module.as_deref(),
        Some("fts5")
    );

    let view = adapter
        .fetch_table_detail(&dsn, "main", "settings_view")
        .await
        .unwrap();
    assert!(
        view.source_ddl()
            .is_some_and(|ddl| ddl.starts_with("CREATE VIEW"))
    );
    assert_eq!(view.kind_info.kind, TableKind::View);
    assert_eq!(
        adapter.generate_ddl(DatabaseType::SQLite, &view),
        view.source_ddl().unwrap()
    );
}
