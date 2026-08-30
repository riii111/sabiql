use crate::adapters::test_support;
use crate::app::ports::outbound::MetadataProvider;
use crate::domain::{TriggerEvent, TriggerTiming};

use super::SqliteAdapter;

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
