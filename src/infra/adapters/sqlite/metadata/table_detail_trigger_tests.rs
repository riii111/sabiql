use crate::adapters::test_support;

use super::SqliteAdapter;

#[tokio::test]
async fn table_detail_preserves_sqlite_trigger_definition_with_unicode_identifier() {
    let definition = "CREATE TRIGGER 監査log AFTER UPDATE OF id ON users FOR EACH ROW WHEN NEW.id <> OLD.id\nBEGIN\n  SELECT NEW.id;\nEND";
    let setup = format!("CREATE TABLE users(id INTEGER PRIMARY KEY);\n{definition};");
    let (_dir, dsn) = test_support::make_sqlite_db(&setup);
    let adapter = SqliteAdapter::new();

    let detail = adapter
        .fetch_table_detail(&dsn, "main", "users")
        .await
        .unwrap();

    assert_eq!(detail.triggers.len(), 1);
    assert_eq!(detail.triggers[0].name, "監査log");
    assert_eq!(detail.triggers[0].definition, definition);
}
