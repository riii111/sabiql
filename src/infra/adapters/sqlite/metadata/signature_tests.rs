use crate::app::ports::outbound::{AccessMode, MetadataProvider, QueryExecutor};

use super::super::SqliteAdapter;

mod table_signatures {
    use crate::adapters::test_support;

    use super::*;

    #[tokio::test]
    async fn all_tables_use_one_sqlite_process() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE organizations(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES organizations(id)
            );
            CREATE INDEX idx_users_org_id ON users(org_id);
            CREATE TABLE events(id INTEGER PRIMARY KEY, user_id INTEGER);
            CREATE INDEX idx_events_user_id ON events(user_id);
            ",
        );
        let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures.len(), 3);
        assert_eq!(process_counter.count(), 1);
    }

    #[tokio::test]
    async fn excludes_views_but_keeps_virtual_tables() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            CREATE VIEW active_users AS SELECT id FROM users;
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            ",
        );
        let adapter = SqliteAdapter::new();

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();
        let names: Vec<_> = signatures
            .iter()
            .map(|signature| signature.name.as_str())
            .collect();

        assert_eq!(names, ["notes_fts", "users"]);
        assert!(!names.contains(&"active_users"));
        assert!(signatures[0].signature.contains("CREATE VIRTUAL TABLE"));
    }

    #[tokio::test]
    async fn change_with_table_shape() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].qualified_name(), "main.users");
        assert!(signatures[0].signature.contains("CREATE TABLE users"));
        assert!(signatures[0].signature.contains("col=id:INTEGER"));
    }

    #[tokio::test]
    async fn include_foreign_key_update_action() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE orgs(id INTEGER PRIMARY KEY);
        CREATE TABLE users(
            org_id INTEGER REFERENCES orgs(id)
                ON DELETE CASCADE
                ON UPDATE SET NULL
        );
        ",
        );
        let adapter = SqliteAdapter::new();

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();
        let signature = signatures
            .iter()
            .find(|signature| signature.name == "users")
            .unwrap();

        assert!(
            signature
                .signature
                .contains("fk=fk_users_0:org_id:orgs:id:CASCADE:SET NULL")
        );
    }

    #[tokio::test]
    async fn unresolved_foreign_key_is_included_in_signature() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            r"
        PRAGMA foreign_keys=OFF;
        CREATE TABLE child(
            org_id INTEGER REFERENCES missing_orgs(id)
        );
        ",
        );
        let adapter = SqliteAdapter::new();

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();
        let signature = signatures
            .iter()
            .find(|signature| signature.name == "child")
            .expect("child table signature");

        assert!(
            signature
                .signature
                .contains("fk=fk_child_0:org_id:missing_orgs:id:NO ACTION:NO ACTION:false")
        );
    }

    #[tokio::test]
    async fn index_desc_and_collation_change_signature() {
        let adapter = SqliteAdapter::new();
        let (_asc_dir, asc_dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(name TEXT);
        CREATE INDEX idx_users_name ON users(name);
        ",
        );
        let (_desc_dir, desc_dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(name TEXT);
        CREATE INDEX idx_users_name ON users(name DESC);
        ",
        );
        let (_binary_dir, binary_dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(name TEXT);
        CREATE INDEX idx_users_name ON users(name);
        ",
        );
        let (_nocase_dir, nocase_dsn) = test_support::make_sqlite_db(
            r"
        CREATE TABLE users(name TEXT);
        CREATE INDEX idx_users_name ON users(name COLLATE NOCASE);
        ",
        );

        let asc_signature = adapter
            .fetch_table_signatures(&asc_dsn)
            .await
            .unwrap()
            .into_iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature;
        let desc_signature = adapter
            .fetch_table_signatures(&desc_dsn)
            .await
            .unwrap()
            .into_iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature;
        let binary_signature = adapter
            .fetch_table_signatures(&binary_dsn)
            .await
            .unwrap()
            .into_iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature;
        let nocase_signature = adapter
            .fetch_table_signatures(&nocase_dsn)
            .await
            .unwrap()
            .into_iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature;

        assert_ne!(asc_signature, desc_signature);
        assert!(
            desc_signature.contains(
                "idx=idx_users_name:name:false:false:false:false:true:true:false:CREATE INDEX idx_users_name ON users(name DESC)"
            )
        );
        assert_ne!(binary_signature, nocase_signature);
        assert!(
            nocase_signature.contains(
                "idx=idx_users_name:name:false:false:false:false:true:false:true:CREATE INDEX idx_users_name ON users(name COLLATE NOCASE)"
            )
        );
    }

    #[tokio::test]
    async fn trigger_change_updates_signature() {
        let setup = r"
        CREATE TABLE users(id INTEGER PRIMARY KEY);
        CREATE TABLE audit(user_id INTEGER);
        ";
        let trigger = r"
        CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN
            INSERT INTO audit(user_id) VALUES (new.id);
        END;
        ";
        let (_dir, dsn) = test_support::make_sqlite_db(setup);
        let adapter = SqliteAdapter::new();

        let before = adapter.fetch_table_signatures(&dsn).await.unwrap();
        let before_signature = before
            .iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature
            .clone();

        adapter
            .execute_adhoc(&dsn, trigger, AccessMode::ReadWrite)
            .await
            .unwrap();

        let after = adapter.fetch_table_signatures(&dsn).await.unwrap();
        let after_signature = &after
            .iter()
            .find(|signature| signature.name == "users")
            .unwrap()
            .signature;

        assert_ne!(before_signature, after_signature.as_str());
        assert!(
            after_signature.contains("trg=users_audit:AFTER:INSERT:CREATE TRIGGER users_audit")
        );
    }
}
