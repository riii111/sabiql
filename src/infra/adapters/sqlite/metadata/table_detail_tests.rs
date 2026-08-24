use crate::app::policy::column::column_read_only_reason;
use crate::app::ports::outbound::{DbOperationError, DdlGenerator, MetadataProvider};
use crate::domain::{
    DatabaseType, FkAction, IndexType, TableKind, TriggerEvent, TriggerTiming, UNRESOLVED_FK_COLUMN,
};

use super::super::super::sqlite3::metadata::RawIndexColumn;
use super::super::SqliteAdapter;
use super::index_key_column_names;

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

mod table_detail {
    use crate::adapters::test_support;

    use super::*;

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
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let result = adapter.fetch_table_detail(&dsn, "other", "users").await;

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    }

    #[tokio::test]
    async fn missing_table_returns_object_missing() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
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
        assert!(detail.columns.iter().any(|column| {
            column.name == "email" && !column.is_nullable() && column.is_unique()
        }));
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
}

mod foreign_keys {
    use crate::adapters::test_support;

    use super::*;

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
}

mod trigger_metadata {
    use crate::adapters::test_support;

    use super::*;

    #[tokio::test]
    async fn table_detail_loads_trigger_without_explicit_timing() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        CREATE TRIGGER users_log INSERT ON users BEGIN SELECT 1; END;
        ",
        );
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.triggers.len(), 1);
        assert_eq!(detail.triggers[0].name, "users_log");
        assert_eq!(detail.triggers[0].timing, TriggerTiming::Before);
        assert_eq!(detail.triggers[0].events, vec![TriggerEvent::Insert]);
    }

    #[tokio::test]
    async fn table_detail_loads_trigger_metadata_from_sqlite_master_sql() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        CREATE TRIGGER IF NOT EXISTS users_audit AFTER INSERT ON users BEGIN SELECT 1; END;
        ",
        );
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.triggers.len(), 1);
        assert_eq!(detail.triggers[0].name, "users_audit");
        assert_eq!(detail.triggers[0].timing, TriggerTiming::After);
        assert_eq!(detail.triggers[0].events, vec![TriggerEvent::Insert]);
        assert!(
            !detail.triggers[0]
                .definition
                .to_ascii_uppercase()
                .contains("TEMP")
        );
    }
}
